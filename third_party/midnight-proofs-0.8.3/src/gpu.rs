//! Accelerator hooks of the prover.
//!
//! This module is a local addition of the 1AM mobile SDK. It is not part of
//! the upstream release of this crate. It backports the hook layer of the GPU
//! prover proposed in `midnightntwrk/midnight-zk#667`, reduced to the two
//! operations that this version of the prover can offer without other
//! changes:
//!
//! - the G1 MSM of a KZG commitment over BLS12-381
//!   ([`GpuHooks::commit_msm`]);
//! - the forward and inverse FFT of
//!   [`EvaluationDomain`](crate::poly::EvaluationDomain) over the BLS12-381
//!   scalar field ([`GpuHooks::fft`], [`GpuHooks::ifft`]).
//!
//! [`warm`] additionally shows the accelerator the base vectors of a set of
//! parameters ahead of the first proof ([`GpuHooks::warm`]).
//!
//! # Hook contract
//!
//! 1. A hook returns the exact value of the stock function on the same
//!    input, or it declines (`None` or `false`).
//! 2. A decline leaves the caller's buffer unchanged. The stock code then
//!    runs on that buffer.
//! 3. A hook draws no random value and touches no transcript.
//! 4. A process installs one hook set with [`install`]. A second call
//!    returns `false` and keeps the first set.
//! 5. With no hook installed the prover runs only the stock code, so its
//!    proofs are the proofs of the upstream release.
//!
//! Only the BLS12-381 scalar field and its G1 group reach a hook: each entry
//! point checks the type first.
#![allow(unsafe_code)]

use std::{any::TypeId, fmt::Debug, sync::OnceLock};

use midnight_curves::{pairing::Engine, Fq, G1Projective};

use crate::poly::kzg::params::ParamsKZG;

/// A scalar as `blst_fr` holds it: four little-endian `u64` limbs in
/// Montgomery form, reduced modulo the BLS12-381 scalar field order.
pub type Scalar = [u64; 4];
/// A Jacobian G1 point as `blst_p1` holds it: `X`, `Y`, `Z`, six
/// little-endian `u64` limbs each, Montgomery form.
pub type G1JacobianLimbs = [u64; 18];

/// The commitment MSM hook: `sum scalars[i] * bases[i]`.
pub type CommitMsmFn = fn(&[Scalar], &[G1JacobianLimbs]) -> Option<G1JacobianLimbs>;
/// The FFT hook: `(a, omega, log_n)`.
pub type FftFn = fn(&mut [Scalar], Scalar, u32) -> bool;
/// The inverse FFT hook: `(a, omega_inv, log_n, divisor)`.
pub type IfftFn = fn(&mut [Scalar], Scalar, u32, Scalar) -> bool;
/// The warm-up hook: one SRS base vector that commitments will use.
pub type WarmFn = fn(&[G1JacobianLimbs]);

// The `midnight-curves` types are `repr(transparent)` wrappers of the `blst`
// types, so a slice of them is a slice of limb arrays. These checks fail the
// build if that ever changes.
const _: () = {
    assert!(std::mem::size_of::<Fq>() == std::mem::size_of::<Scalar>());
    assert!(std::mem::align_of::<Fq>() == std::mem::align_of::<Scalar>());
    assert!(std::mem::size_of::<G1Projective>() == std::mem::size_of::<G1JacobianLimbs>());
    assert!(std::mem::align_of::<G1Projective>() == std::mem::align_of::<G1JacobianLimbs>());
};

/// The accelerator functions. Each one is optional, and each one may decline.
#[derive(Default, Clone, Copy, Debug)]
pub struct GpuHooks {
    /// `sum scalars[i] * bases[i]` (equal lengths) for a KZG commitment. The
    /// bases are a prefix of an SRS base vector. `None`: declined.
    pub commit_msm: Option<CommitMsmFn>,
    /// The FFT of `a` (length `2^log_n`) at the root `omega`, in natural
    /// order, in place. `false`: declined, and `a` is unchanged.
    pub fft: Option<FftFn>,
    /// The FFT of `a` at `omega_inv`, then each element times `divisor`, in
    /// place. `false`: declined, and `a` is unchanged.
    pub ifft: Option<IfftFn>,
    /// Called by [`warm`] with a base vector that later commitments will
    /// use, so that the accelerator can prepare its copy outside a proof. It
    /// computes nothing for the prover and may do nothing.
    pub warm: Option<WarmFn>,
}

static HOOKS: OnceLock<GpuHooks> = OnceLock::new();

/// Installs the hooks of this process. Returns `false`, and changes nothing,
/// when hooks are installed already.
pub fn install(hooks: GpuHooks) -> bool {
    HOOKS.set(hooks).is_ok()
}

/// The installed hooks, if any.
pub fn installed() -> Option<GpuHooks> {
    HOOKS.get().copied()
}

/// Shows the installed accelerator the two base vectors of `params` ahead of
/// the first proof. Does nothing without an installed accelerator, and for a
/// curve other than BLS12-381.
pub fn warm<E: Engine + Debug>(params: &ParamsKZG<E>)
where
    E::G1: 'static,
{
    let Some(f) = HOOKS.get().and_then(|h| h.warm) else {
        return;
    };
    if !same::<E::G1, G1Projective>() {
        return;
    }
    for bases in [&params.g[..], &params.g_lagrange[..]] {
        // SAFETY: `E::G1` is `G1Projective` (checked above), a transparent
        // `blst_p1` with the size and alignment of `G1JacobianLimbs`.
        f(unsafe { &*(bases as *const [E::G1] as *const [G1JacobianLimbs]) });
    }
}

#[inline]
fn same<A: 'static, B: 'static>() -> bool {
    TypeId::of::<A>() == TypeId::of::<B>()
}

/// The commitment MSM hook. `None`: not taken, and the stock code runs.
pub(crate) fn commit_msm<S: 'static, P: 'static>(scalars: &[S], bases: &[P]) -> Option<P> {
    let f = HOOKS.get()?.commit_msm?;
    if !same::<S, Fq>() || !same::<P, G1Projective>() || scalars.len() != bases.len() {
        return None;
    }
    // SAFETY: `S` is `Fq`, a transparent `blst_fr` (`[u64; 4]`), and `P` is
    // `G1Projective`, a transparent `blst_p1` (`[u64; 18]`): both checked
    // above, with the sizes and alignments asserted at build time.
    let (s, b) = unsafe {
        (
            &*(scalars as *const [S] as *const [Scalar]),
            &*(bases as *const [P] as *const [G1JacobianLimbs]),
        )
    };
    let r = f(s, b)?;
    // SAFETY: `P` is `G1Projective` (checked above), which has the layout of
    // `G1JacobianLimbs`; the hook contract makes `r` the limbs of the point
    // the stock MSM returns.
    Some(unsafe { std::mem::transmute_copy::<G1JacobianLimbs, P>(&r) })
}

/// The FFT hook. `false`: not taken, `a` is unchanged and the stock code
/// runs.
pub(crate) fn fft<F: 'static>(a: &mut [F], omega: &F, log_n: u32) -> bool {
    let Some(f) = HOOKS.get().and_then(|h| h.fft) else {
        return false;
    };
    if !same::<F, Fq>() || a.len() as u64 != 1u64 << log_n {
        return false;
    }
    // SAFETY: `F` is `Fq` (checked above), a transparent `blst_fr` with the
    // size and alignment of `Scalar`.
    let (a, omega) = unsafe {
        (
            &mut *(a as *mut [F] as *mut [Scalar]),
            *(omega as *const F as *const Scalar),
        )
    };
    f(a, omega, log_n)
}

/// The inverse FFT hook (the transform at the inverse root, then the scaling
/// by `divisor`). `false`: not taken, `a` is unchanged and the stock code
/// runs.
pub(crate) fn ifft<F: 'static>(a: &mut [F], omega_inv: &F, log_n: u32, divisor: &F) -> bool {
    let Some(f) = HOOKS.get().and_then(|h| h.ifft) else {
        return false;
    };
    if !same::<F, Fq>() || a.len() as u64 != 1u64 << log_n {
        return false;
    }
    // SAFETY: as in `fft`.
    let (a, omega_inv, divisor) = unsafe {
        (
            &mut *(a as *mut [F] as *mut [Scalar]),
            *(omega_inv as *const F as *const Scalar),
            *(divisor as *const F as *const Scalar),
        )
    };
    f(a, omega_inv, log_n, divisor)
}
