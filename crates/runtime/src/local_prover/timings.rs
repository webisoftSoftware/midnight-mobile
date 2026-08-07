//! Per-proof stage instrumentation (plan Phase 1).
//!
//! Off by default. The cache-candidate measurement duplicates real key-material decode and init
//! work, so it must be requested explicitly via `set_profiling`; when disabled this module costs
//! only a handful of `Instant::now()` calls on the prove path.

use std::io::Cursor;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Instant;

use midnight_serialize::tagged_deserialize;
use midnight_transient_crypto::proofs::{ProvingKeyMaterial, VerifierKey, Zkir};
use midnight_zkir::IrSource;
use serde::Serialize;
use std::collections::VecDeque;

use super::{LocalProverError, MAX_TIMING_SAMPLES, PROFILE_STAGES};

static TIMING_SAMPLES: Mutex<VecDeque<ProofStageTiming>> = Mutex::new(VecDeque::new());

/// One drained sample of per-proof stage timings (Phase 1 instrumentation). Contains no secrets:
/// only a key location string, byte counts, and durations. Field names are camelCase to match the
/// JSON the platform layer consumes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProofStageTiming {
    pub(super) key_location: String,
    pub(super) request_bytes: usize,
    pub(super) k: Option<u8>,
    pub(super) deserialize_request_micros: u64,
    pub(super) select_material_micros: u64,
    pub(super) prove_call_micros: u64,
    pub(super) serialize_response_micros: u64,
    pub(super) ir_load_micros: Option<u64>,
    pub(super) prover_key_init_micros: Option<u64>,
    pub(super) verifier_key_init_micros: Option<u64>,
}

pub(super) struct ProveStageDurations {
    pub(super) deserialize_request_micros: u64,
    pub(super) select_material_micros: u64,
    pub(super) prove_call_micros: u64,
    pub(super) serialize_response_micros: u64,
}

struct CacheCandidateTiming {
    k: u8,
    ir_load_micros: u64,
    prover_key_init_micros: u64,
    verifier_key_init_micros: u64,
}

pub(super) fn stage_micros(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Times exactly the per-proof work a typed initialized-circuit cache (plan Phase 6) would
/// eliminate: IR load, prover-key load + init, verifier-key deserialize + init. Duplicates real
/// decode/init work, so callers must only run this when profiling is enabled.
fn measure_cache_candidate_stages(material: &ProvingKeyMaterial) -> Option<CacheCandidateTiming> {
    let ir_start = Instant::now();
    let ir = IrSource::load_from_tagged(Cursor::new(&material.ir_source)).ok()?;
    let ir_load_micros = stage_micros(ir_start);

    let key_start = Instant::now();
    let prover_key =
        IrSource::load_prover_key_from_tagged(Cursor::new(&material.prover_key)).ok()?;
    prover_key.init().ok()?;
    let prover_key_init_micros = stage_micros(key_start);

    let verifier_start = Instant::now();
    let verifier_key: VerifierKey = tagged_deserialize(&mut &material.verifier_key[..]).ok()?;
    verifier_key.init().ok()?;
    let verifier_key_init_micros = stage_micros(verifier_start);

    Some(CacheCandidateTiming {
        k: ir.k(),
        ir_load_micros,
        prover_key_init_micros,
        verifier_key_init_micros,
    })
}

pub(super) fn record_timing_sample(sample: ProofStageTiming) {
    let mut samples = TIMING_SAMPLES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if samples.len() >= MAX_TIMING_SAMPLES {
        samples.pop_front();
    }
    samples.push_back(sample);
}

/// Only called when profiling is enabled (`cached_material` is `Some` exactly then). Runs the
/// Phase 6 decision-gate measurement and records the combined sample.
pub(super) fn maybe_record_prove_timing(
    cached_material: Option<ProvingKeyMaterial>,
    key_location: String,
    request_bytes: usize,
    stages: ProveStageDurations,
) {
    let Some(material) = cached_material else {
        return;
    };
    let cache_candidate = measure_cache_candidate_stages(&material);
    record_timing_sample(ProofStageTiming {
        key_location,
        request_bytes,
        k: cache_candidate.as_ref().map(|timing| timing.k),
        deserialize_request_micros: stages.deserialize_request_micros,
        select_material_micros: stages.select_material_micros,
        prove_call_micros: stages.prove_call_micros,
        serialize_response_micros: stages.serialize_response_micros,
        ir_load_micros: cache_candidate.as_ref().map(|timing| timing.ir_load_micros),
        prover_key_init_micros: cache_candidate
            .as_ref()
            .map(|timing| timing.prover_key_init_micros),
        verifier_key_init_micros: cache_candidate
            .as_ref()
            .map(|timing| timing.verifier_key_init_micros),
    });
}

/// Sets whether per-proof stage timing (Phase 1 instrumentation) is collected. Disabled by
/// default: the decision-gate measurement it enables duplicates real key-material decode/init
/// work, so it must be explicitly requested rather than run on every proof.
pub(crate) fn set_profiling(enabled: bool) {
    PROFILE_STAGES.store(enabled, Ordering::Release);
}

/// Drains all recorded stage-timing samples as a JSON array (UTF-8 bytes), oldest first. Returns
/// `[]` when empty. Draining is destructive: a second call with no new proofs in between yields
/// `[]` again.
pub(crate) fn take_timings() -> Result<Vec<u8>, LocalProverError> {
    let samples: Vec<ProofStageTiming> = {
        let mut guard = TIMING_SAMPLES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.drain(..).collect()
    };
    serde_json::to_vec(&samples).map_err(|_| LocalProverError::NativeInternal)
}

/// Direct access to the sample queue, for tests that need to seed or clear it.
#[cfg(test)]
pub(super) fn test_samples() -> &'static Mutex<VecDeque<ProofStageTiming>> {
    &TIMING_SAMPLES
}
