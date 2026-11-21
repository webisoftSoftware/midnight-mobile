//! SRS base tables for the device.
//!
//! The prover commits with `sum scalars[i] * bases[i]`, where `bases` is a
//! prefix of one of the base vectors of its KZG parameters. This prover keeps
//! those vectors as Jacobian points and may load them again for every proof,
//! so their addresses say nothing. The device wants affine points that it can
//! keep between calls.
//!
//! A [`Table`] is the affine copy of one base vector, registered with the
//! Metal backend. A call is served from a table only after every point of the
//! caller's slice was compared with it, so a table can never stand in for
//! different bases. Tables are found by content, and the least recently used
//! ones are dropped when the cache is over its budget.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rayon::prelude::*;

use crate::{G1AffineLimbs, G1JacobianLimbs};

/// Points the cache may hold in total: 48 MiB of affine points, and the same
/// again on the device.
const MAX_CACHED_POINTS: usize = 1 << 19;
/// Tables the cache may hold.
const MAX_TABLES: usize = 16;
/// Points compared per parallel task.
const COMPARE_CHUNK: usize = 1 << 12;

/// The Montgomery form of one in the BLS12-381 base field: the `Z` limbs of a
/// normalized Jacobian point.
const FP_ONE: [u64; 6] = [
    0x7609_0000_0002_fffd,
    0xebf4_000b_c40c_0002,
    0x5f48_9857_53c7_58ba,
    0x77ce_5853_7052_5745,
    0x5c07_1a97_a256_ec6d,
    0x15f6_5ec3_fa80_e493,
];

static TABLES: Mutex<Vec<Arc<Table>>> = Mutex::new(Vec::new());
static CLOCK: AtomicU64 = AtomicU64::new(0);

/// The affine copy of one base vector.
pub(crate) struct Table {
    // Declared first so that it is dropped first: the registration refers to
    // `points` for as long as it lives.
    _registration: midnight_metal::SrsHandle,
    points: Box<[G1AffineLimbs]>,
    /// The registered Jacobian points, kept only when they were not all
    /// normalized (`Z = 1`). `None`: the caller's points are compared with
    /// `points` directly.
    source: Option<Box<[G1JacobianLimbs]>>,
    last_used: AtomicU64,
}

impl Table {
    /// The first `len` affine points; `None` when the table is shorter.
    pub(crate) fn prefix(&self, len: usize) -> Option<&[G1AffineLimbs]> {
        self.points.get(..len)
    }

    /// Whether `bases` is a prefix of the vector this table was made from.
    /// Every point is compared.
    fn serves(&self, bases: &[G1JacobianLimbs]) -> bool {
        let len = bases.len();
        if len == 0 || len > self.points.len() {
            return false;
        }
        match &self.source {
            Some(source) => {
                bases[0] == source[0]
                    && bases
                        .par_chunks(COMPARE_CHUNK)
                        .zip(source[..len].par_chunks(COMPARE_CHUNK))
                        .all(|(a, b)| a == b)
            }
            None => {
                is_normalized_copy(&bases[0], &self.points[0])
                    && bases
                        .par_chunks(COMPARE_CHUNK)
                        .zip(self.points[..len].par_chunks(COMPARE_CHUNK))
                        .all(|(jacobian, affine)| {
                            jacobian
                                .iter()
                                .zip(affine)
                                .all(|(j, a)| is_normalized_copy(j, a))
                        })
            }
        }
    }

    fn touch(&self) {
        self.last_used
            .store(CLOCK.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
    }
}

/// Whether `jacobian` is `affine` with `Z = 1`, or both are the point at
/// infinity in their canonical all-zero form.
fn is_normalized_copy(jacobian: &G1JacobianLimbs, affine: &G1AffineLimbs) -> bool {
    if jacobian[..12] != affine[..] {
        return false;
    }
    jacobian[12..] == FP_ONE || jacobian.iter().all(|&limb| limb == 0)
}

fn tables() -> MutexGuard<'static, Vec<Arc<Table>>> {
    TABLES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The table that serves an MSM over `bases`, made and registered if no
/// cached table does. `None`: the bases cannot be cached, and the MSM runs on
/// the CPU.
pub(crate) fn serving(bases: &[G1JacobianLimbs]) -> Option<Arc<Table>> {
    if bases.is_empty() || bases.len() > MAX_CACHED_POINTS {
        return None;
    }
    // Pin the candidates, then compare outside the lock.
    let candidates: Vec<Arc<Table>> = tables()
        .iter()
        .filter(|table| table.points.len() >= bases.len())
        .cloned()
        .collect();
    if let Some(table) = candidates.into_iter().find(|table| table.serves(bases)) {
        table.touch();
        return Some(table);
    }
    let table = Arc::new(build(bases)?);
    table.touch();
    Some(insert(table))
}

/// Makes and registers the table of `bases`.
fn build(bases: &[G1JacobianLimbs]) -> Option<Table> {
    let normalized = bases.par_chunks(COMPARE_CHUNK).all(|chunk| {
        chunk
            .iter()
            .all(|p| p[12..] == FP_ONE || p.iter().all(|&limb| limb == 0))
    });
    let (points, source) = if normalized {
        let points: Vec<G1AffineLimbs> = bases
            .par_iter()
            .map(|jacobian| {
                let mut affine = [0u64; 12];
                affine.copy_from_slice(&jacobian[..12]);
                affine
            })
            .collect();
        (points.into_boxed_slice(), None)
    } else {
        (to_affine(bases)?, Some(bases.to_vec().into_boxed_slice()))
    };
    // The registration checks that every point is in G1; an MSM over a prefix
    // with a point outside G1 declines in the backend.
    let registration = midnight_metal::register_srs(&points);
    Some(Table {
        _registration: registration,
        points,
        source,
        last_used: AtomicU64::new(0),
    })
}

/// Batch conversion of Jacobian points that are not normalized.
fn to_affine(bases: &[G1JacobianLimbs]) -> Option<Box<[G1AffineLimbs]>> {
    let mut points = vec![[0u64; 12]; bases.len()];
    // blst takes an array of pointers; a null entry means "continue after the
    // previous one", so two entries describe one contiguous slice.
    let sources: [*const blst::blst_p1; 2] = [bases.as_ptr().cast(), std::ptr::null()];
    // `G1JacobianLimbs` and `G1AffineLimbs` have the size and alignment of
    // `blst_p1` and `blst_p1_affine` (`repr(C)` structs of `[u64; 6]` fields,
    // asserted below), and `points` holds `bases.len()` elements.
    // SAFETY: Both slices have the layouts blst reads and writes, and outlive the call.
    unsafe {
        blst::blst_p1s_to_affine(points.as_mut_ptr().cast(), sources.as_ptr(), bases.len());
    }
    Some(points.into_boxed_slice())
}

const _: () = {
    assert!(size_of::<blst::blst_p1>() == size_of::<G1JacobianLimbs>());
    assert!(align_of::<blst::blst_p1>() == align_of::<G1JacobianLimbs>());
    assert!(size_of::<blst::blst_p1_affine>() == size_of::<G1AffineLimbs>());
    assert!(align_of::<blst::blst_p1_affine>() == align_of::<G1AffineLimbs>());
};

/// Adds `table` to the cache and returns the table to use: `table`, or a
/// cached one that already covers the same points (another thread built it in
/// the meantime).
fn insert(table: Arc<Table>) -> Arc<Table> {
    let len = table.points.len();
    let mut cached = tables();
    if let Some(existing) = cached.iter().find(|other| covers(other, &table)) {
        existing.touch();
        return Arc::clone(existing);
    }
    // A cached table that is a prefix of the new one is now redundant.
    cached.retain(|other| !covers(&table, other));
    while !cached.is_empty()
        && (cached.len() >= MAX_TABLES
            || cached.iter().map(|other| other.points.len()).sum::<usize>() + len
                > MAX_CACHED_POINTS)
    {
        let oldest = cached
            .iter()
            .enumerate()
            .min_by_key(|(_, other)| other.last_used.load(Ordering::Relaxed))
            .map(|(index, _)| index);
        match oldest {
            Some(index) => {
                cached.swap_remove(index);
            }
            None => break,
        }
    }
    cached.push(Arc::clone(&table));
    table
}

/// Whether `longer` holds every point of `shorter` as a prefix, from the
/// same kind of source.
fn covers(longer: &Table, shorter: &Table) -> bool {
    let len = shorter.points.len();
    longer.points.len() >= len
        && longer.points[..len] == shorter.points[..]
        && match (&longer.source, &shorter.source) {
            (None, None) => true,
            (Some(a), Some(b)) => a[..len] == b[..],
            _ => false,
        }
}

/// `(tables, points)` in the cache.
pub(crate) fn usage() -> (u64, u64) {
    let cached = tables();
    let points: usize = cached.iter().map(|table| table.points.len()).sum();
    (cached.len() as u64, points as u64)
}

/// Empties the cache. A table in use by a running MSM is freed when that MSM
/// ends.
pub(crate) fn clear() {
    let dropped = std::mem::take(&mut *tables());
    // Outside the lock: dropping a table unregisters it from the device.
    drop(dropped);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fp_one_is_the_z_of_a_normalized_point() {
        let mut point = blst::blst_p1::default();
        // SAFETY: Both pointers are valid; `BLS12_381_G1` is blst's affine generator.
        unsafe { blst::blst_p1_from_affine(&mut point, &blst::BLS12_381_G1) };
        assert_eq!(point.z.l, FP_ONE);
    }

    #[test]
    fn normalized_copy_checks_every_limb() {
        let mut jacobian = [7u64; 18];
        jacobian[12..].copy_from_slice(&FP_ONE);
        let affine = [7u64; 12];
        assert!(is_normalized_copy(&jacobian, &affine));
        let mut other = affine;
        other[11] ^= 1;
        assert!(!is_normalized_copy(&jacobian, &other));
        let mut scaled = jacobian;
        scaled[17] ^= 1;
        assert!(!is_normalized_copy(&scaled, &affine));
        assert!(is_normalized_copy(&[0u64; 18], &[0u64; 12]));
    }
}
