//! GPU acceleration of the local prover.
//!
//! Off until the host asks for it with
//! `midnight_mobile_local_prover_set_accelerator(true)`. Until then nothing is
//! installed and the prover is the stock CPU prover. Once it is on, the
//! commitment MSMs and the FFTs of a proof run on the device when the device
//! takes them; an operation the device declines runs on the CPU. Either way a
//! proof is the proof the CPU prover makes from the same randomness (see
//! `midnight_mobile_prover_accel`).
//!
//! On a target without an accelerator the switch is accepted and the status
//! reports `unsupported`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use midnight_mobile_prover_accel as accel;
use serde::Serialize;

use super::ffi::{LocalProverResponse, guarded_response};
use super::{LocalProverError, MemoryRegistry};

/// Whether the host asked for the accelerator.
static REQUESTED: AtomicBool = AtomicBool::new(false);
/// Set when a proof failed with the device and then succeeded on the CPU. The
/// accelerator stays off until the host turns it on again.
static TRIPPED: AtomicBool = AtomicBool::new(false);
/// Proofs that failed with the device and were made on the CPU instead.
static CPU_RETRIES: AtomicU64 = AtomicU64::new(0);

/// Turns the accelerator on or off. Turning it on installs it on first use;
/// that never fails: without a usable device the prover stays on the CPU and
/// the status says why. Turning it on also re-arms it after a trip.
pub(crate) fn set_accelerator(enabled: bool) {
    REQUESTED.store(enabled, Ordering::Release);
    if enabled {
        TRIPPED.store(false, Ordering::Release);
        accel::install();
        accel::set_enabled(true);
        // The registry may have been configured while the accelerator was off.
        let current = super::registry_slot()
            .lock()
            .ok()
            .and_then(|slot| slot.value.clone());
        if let Some(registry) = current {
            warm(&registry);
        }
    } else {
        // Off first, so no operation that is under way registers a table again
        // after the tables were freed.
        accel::set_enabled(false);
        accel::trim_memory();
    }
}

/// Makes a proof with `attempt`, which may run more than once.
///
/// The ledger verifies every proof against the verifier key before it returns
/// it, so a wrong device result can never leave as a proof: it fails the
/// attempt. If an attempt fails after the device worked on it, the proof is
/// attempted once more with the accelerator off. When that succeeds, the
/// device was at fault: the accelerator stays off (see `TRIPPED`) and the
/// caller gets its proof. When it fails too, the failure is the request's own
/// and the accelerator is put back.
pub(super) fn prove<T>(
    mut attempt: impl FnMut() -> Result<T, LocalProverError>,
) -> (Result<T, LocalProverError>, AcceleratorUse) {
    let scope = ProofScope::begin();
    let first = attempt();
    let usage = scope.usage();
    let used_device = scope.is_active() && usage.gpu_operations > 0;
    drop(scope);
    if first.is_ok() || !used_device {
        return (first, usage);
    }
    accel::set_enabled(false);
    let retry = attempt();
    if retry.is_ok() {
        TRIPPED.store(true, Ordering::Release);
        CPU_RETRIES.fetch_add(1, Ordering::Relaxed);
        accel::release_tables();
    } else {
        accel::set_enabled(wanted());
    }
    (retry, usage)
}

/// Whether the accelerator should be on: requested and not tripped.
fn wanted() -> bool {
    REQUESTED.load(Ordering::Acquire) && !TRIPPED.load(Ordering::Acquire)
}

/// Prepares the device copies of the proving parameters that the registry's
/// circuits use, so that the first proof does not pay for them. Does nothing
/// while the accelerator is not in use. Circuits a request supplies itself are
/// not known here; their parameters are prepared by their first proof.
pub(super) fn warm(registry: &MemoryRegistry) {
    if !(wanted() && accel::is_active()) {
        return;
    }
    let mut sizes: Vec<u8> = registry
        .circuits
        .values()
        .filter_map(|material| super::circuit_size(&material.ir_source).ok())
        .collect();
    // Largest first: the monomial bases of a smaller size are a prefix of a
    // larger one's and then share its table.
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes.dedup();
    for k in sizes {
        if let Some(params) = registry.params.get(&k) {
            midnight_proofs::gpu::warm(params.as_ref());
        }
    }
}

/// Frees the device copies of the proving parameters. Called when the
/// registry that owns the parameters is replaced or closed.
pub(super) fn release_tables() {
    accel::release_tables();
}

/// What the accelerator did for one proof, or since the process started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AcceleratorUse {
    /// Commitment MSMs and FFTs the device computed.
    pub(super) gpu_operations: u64,
    /// Commitment MSMs and FFTs offered to the device that ran on the CPU.
    pub(super) cpu_operations: u64,
    /// Device time of the served operations.
    pub(super) device_micros: u64,
}

impl AcceleratorUse {
    fn of(counters: &accel::Counters) -> Self {
        Self {
            gpu_operations: counters.gpu_operations(),
            cpu_operations: counters.msm_cpu + counters.fft_cpu + counters.ifft_cpu,
            device_micros: counters.device_nanos / 1_000,
        }
    }
}

/// One proof's use of the accelerator. While it lives, device buffers are
/// reused between the proof's operations; dropping the last live scope zeroes
/// them.
struct ProofScope {
    request: Option<accel::Request>,
    before: accel::Counters,
}

impl ProofScope {
    fn begin() -> Self {
        let active = wanted() && accel::is_active();
        Self {
            request: active.then(accel::Request::begin),
            before: accel::counters(),
        }
    }

    /// Whether the accelerator was on when this proof began.
    fn is_active(&self) -> bool {
        self.request.is_some()
    }

    /// The accelerator's work since `begin`. The counters are shared by the
    /// process, so with several proofs in flight this includes their work.
    fn usage(&self) -> AcceleratorUse {
        AcceleratorUse::of(&accel::counters().since(&self.before))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceleratorStatus {
    /// Whether the host asked for the accelerator.
    requested: bool,
    /// `not-requested`, `active`, `unsupported`, `no-device` or `hooks-taken`.
    availability: &'static str,
    /// Whether proofs use the device now.
    active: bool,
    /// Whether a device fault turned the accelerator off (see `prove`).
    tripped: bool,
    /// Proofs that failed with the device and were made on the CPU instead.
    cpu_retries: u64,
    device: Option<String>,
    #[serde(flatten)]
    total: AcceleratorUse,
    tables: u64,
    table_points: u64,
}

/// The accelerator's state as a JSON object (UTF-8 bytes).
pub(crate) fn accelerator_status() -> Result<Vec<u8>, LocalProverError> {
    let requested = REQUESTED.load(Ordering::Acquire);
    // `install` is idempotent and was already run if the host ever asked.
    let availability = if requested {
        accel::install().name()
    } else {
        "not-requested"
    };
    let counters = accel::counters();
    let status = AcceleratorStatus {
        requested,
        availability,
        active: wanted() && accel::is_active(),
        tripped: TRIPPED.load(Ordering::Acquire),
        cpu_retries: CPU_RETRIES.load(Ordering::Relaxed),
        device: if requested {
            accel::device_name()
        } else {
            None
        },
        total: AcceleratorUse::of(&counters),
        tables: counters.tables,
        table_points: counters.table_points,
    };
    serde_json::to_vec(&status).map_err(|_| LocalProverError::NativeInternal)
}

/// Turns GPU acceleration of proving on or off. Off by default. Always
/// succeeds: on a device without a usable GPU the prover keeps using the CPU,
/// and `midnight_mobile_local_prover_accelerator_status` reports why.
#[unsafe(no_mangle)]
pub extern "C" fn midnight_mobile_local_prover_set_accelerator(enabled: bool) -> i32 {
    super::ffi::guarded_code(|| {
        set_accelerator(enabled);
        Ok(())
    })
}

/// Gives the accelerator's reusable memory back to the system: the device copies of the
/// proving parameters and the idle device buffers. For the host's memory-pressure
/// callback. Safe at any time, also while proofs run and when the accelerator is off;
/// the next accelerated proof allocates what it needs again.
#[unsafe(no_mangle)]
pub extern "C" fn midnight_mobile_local_prover_trim_memory() -> i32 {
    super::ffi::guarded_code(|| {
        accel::trim_memory();
        Ok(())
    })
}

/// Writes the accelerator's state as a UTF-8 JSON object through the usual
/// `LocalProverResponse` ownership contract; free with
/// `midnight_mobile_local_prover_free`.
///
/// # Safety
///
/// `output` must remain valid for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_mobile_local_prover_accelerator_status(
    output: *mut LocalProverResponse,
) -> i32 {
    if output.is_null() {
        return LocalProverError::InvalidRequest.ffi_code();
    }
    // SAFETY: Output pointer was checked non-null and caller guarantees writable storage.
    let output = unsafe { &mut *output };
    *output = LocalProverResponse::default();
    guarded_response(output, accelerator_status)
}

#[cfg(test)]
mod tests;
