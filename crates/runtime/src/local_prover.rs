use std::collections::HashMap;
use std::io::{self, Cursor};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use midnight_coin_structure::coin::{Info as CoinInfo, QualifiedInfo as QualifiedCoinInfo};
use midnight_ledger::structure::{ProofPreimageVersioned, ProofVersioned};
use midnight_proofs::poly::commitment::Params;
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use midnight_storage::db::InMemoryDB;
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::merkle_tree::MerkleTree;
use midnight_transient_crypto::proofs::{
    KeyLocation, ParamsProver, ParamsProverProvider, ProvingKeyMaterial, Resolver, VerifierKey,
    WrappedIr, Zkir,
};
use midnight_zkir::IrSource;
use midnight_zswap::{Input, Output};
use rand::SeedableRng;
use rand::rngs::{OsRng, StdRng};
use rayon::{ThreadPool, ThreadPoolBuilder};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

mod ffi;
mod timings;

use timings::{ProveStageDurations, maybe_record_prove_timing, stage_micros};
pub(crate) use timings::{set_profiling, take_timings};

const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const MAX_ARTIFACT_BYTES: usize = 512 * 1024 * 1024;
const MAX_TOTAL_ARTIFACT_BYTES: usize = 2 * 1024 * 1024 * 1024;
const MAX_PARAMETER_COUNT: usize = 32;
const MAX_CIRCUIT_COUNT: usize = 256;
const MAX_KEY_LOCATION_BYTES: usize = 1_024;
// Four threads match the measured Android device's performance-core cluster
// while keeping mobile proof parallelism bounded for memory and thermals.
const PROVER_THREAD_COUNT: usize = 4;
// Bounded admission (Phase 2): shared permits (check/prove) are capped by
// MAX_CONCURRENT_PROOFS, exclusive permits (configure/close) always exclude all
// others. Waiters queue on PROVER_GATE up to this total wait budget before a
// genuine PROVER_BUSY is returned.
const MIN_CONCURRENT_PROOFS: usize = 1;
const MAX_CONCURRENT_PROOFS_CEILING: usize = 4;
const DEFAULT_CONCURRENT_PROOFS: usize = 2;
const PROVER_WAIT_BUDGET: Duration = Duration::from_secs(180);
const MAX_TIMING_SAMPLES: usize = 32;

static MAX_CONCURRENT_PROOFS: AtomicUsize = AtomicUsize::new(DEFAULT_CONCURRENT_PROOFS);
// Phase 5 cooperative cancellation: a batch worker that has not yet started its individual
// `run_prove` call abandons it once it observes a newer epoch than the one captured when the
// batch began. This cannot interrupt a proof already inside `preimage.prove(...)` -- that call is
// pinned upstream ledger code with no cancellation token -- so it bounds the unkillable window to
// one in-flight proof per admitted permit rather than eliminating it.
static CANCEL_EPOCH: AtomicU64 = AtomicU64::new(0);
static PROVER_POOL: OnceLock<Result<ThreadPool, rayon::ThreadPoolBuildError>> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<RegistrySlot>> = OnceLock::new();
static PROFILE_STAGES: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static PROVER_TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LocalProverError {
    #[error("INVALID_REQUEST")]
    InvalidRequest,
    #[error("UNSUPPORTED_CIRCUIT")]
    UnsupportedCircuit,
    #[error("INTEGRITY_CHECK_FAILED")]
    IntegrityCheckFailed,
    #[error("PROVER_BUSY")]
    ProverBusy,
    #[error("RESOURCE_PREFLIGHT_FAILED")]
    ResourcePreflightFailed,
    #[error("PROOF_FAILED")]
    ProofFailed,
    #[error("INVALID_CONFIGURATION")]
    InvalidConfiguration,
    #[error("STALE_REGISTRY")]
    StaleRegistry,
    #[error("CHECK_FAILED")]
    CheckFailed,
    #[error("NATIVE_INTERNAL")]
    NativeInternal,
}

pub(crate) struct ParameterArtifact<'a> {
    pub(crate) k: u8,
    pub(crate) bytes: &'a [u8],
    pub(crate) sha256: &'a [u8],
}

pub(crate) struct CircuitArtifact<'a> {
    pub(crate) key_location: &'a str,
    pub(crate) prover_key: &'a [u8],
    pub(crate) prover_key_sha256: &'a [u8],
    pub(crate) verifier_key: &'a [u8],
    pub(crate) verifier_key_sha256: &'a [u8],
    pub(crate) ir: &'a [u8],
    pub(crate) ir_sha256: &'a [u8],
}

/// Writer-preferring read/write gate: `run_check`/`run_prove` take shared permits (bounded by
/// `MAX_CONCURRENT_PROOFS`), `configure_registry`/`close_registry` take the exclusive permit. A
/// waiting exclusive request blocks new shared acquisitions so configure/close cannot be starved
/// by a stream of proofs. All waits are bounded by `PROVER_WAIT_BUDGET`; exhausting the budget is
/// the only remaining path to `PROVER_BUSY`.
#[derive(Default)]
struct ProverGateState {
    readers: usize,
    writer_active: bool,
    writers_waiting: usize,
}

struct ProverGate {
    state: Mutex<ProverGateState>,
    condvar: Condvar,
}

static PROVER_GATE: ProverGate = ProverGate {
    state: Mutex::new(ProverGateState {
        readers: 0,
        writer_active: false,
        writers_waiting: 0,
    }),
    condvar: Condvar::new(),
};

fn lock_gate_state() -> MutexGuard<'static, ProverGateState> {
    PROVER_GATE
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Waits on `PROVER_GATE`'s condvar until `deadline`, tolerating spurious wakeups and mutex
/// poisoning. Returns the reacquired guard and whether the deadline was reached.
fn wait_on_gate(
    guard: MutexGuard<'static, ProverGateState>,
    deadline: Instant,
) -> (MutexGuard<'static, ProverGateState>, bool) {
    let now = Instant::now();
    if now >= deadline {
        return (guard, true);
    }
    match PROVER_GATE.condvar.wait_timeout(guard, deadline - now) {
        Ok((guard, result)) => (guard, result.timed_out()),
        Err(poisoned) => {
            let (guard, result) = poisoned.into_inner();
            (guard, result.timed_out())
        }
    }
}

struct SharedProverPermit;

impl SharedProverPermit {
    fn acquire() -> Result<Self, LocalProverError> {
        let deadline = Instant::now() + PROVER_WAIT_BUDGET;
        let mut guard = lock_gate_state();
        loop {
            let max = MAX_CONCURRENT_PROOFS.load(Ordering::Acquire);
            if !guard.writer_active && guard.writers_waiting == 0 && guard.readers < max {
                guard.readers += 1;
                return Ok(Self);
            }
            let timed_out;
            (guard, timed_out) = wait_on_gate(guard, deadline);
            if timed_out {
                return Err(LocalProverError::ProverBusy);
            }
        }
    }
}

impl Drop for SharedProverPermit {
    fn drop(&mut self) {
        let mut guard = lock_gate_state();
        guard.readers = guard.readers.saturating_sub(1);
        drop(guard);
        PROVER_GATE.condvar.notify_all();
    }
}

struct ExclusiveProverPermit;

impl ExclusiveProverPermit {
    fn acquire() -> Result<Self, LocalProverError> {
        let deadline = Instant::now() + PROVER_WAIT_BUDGET;
        let mut guard = lock_gate_state();
        guard.writers_waiting += 1;
        let outcome = loop {
            if !guard.writer_active && guard.readers == 0 {
                guard.writer_active = true;
                break Ok(Self);
            }
            let timed_out;
            (guard, timed_out) = wait_on_gate(guard, deadline);
            if timed_out {
                break Err(LocalProverError::ProverBusy);
            }
        };
        guard.writers_waiting = guard.writers_waiting.saturating_sub(1);
        drop(guard);
        if outcome.is_err() {
            PROVER_GATE.condvar.notify_all();
        }
        outcome
    }
}

impl Drop for ExclusiveProverPermit {
    fn drop(&mut self) {
        let mut guard = lock_gate_state();
        guard.writer_active = false;
        drop(guard);
        PROVER_GATE.condvar.notify_all();
    }
}

/// Sets the maximum number of shared (check/prove) permits, clamped to `1..=4`.
pub(crate) fn set_max_concurrency(limit: usize) -> usize {
    let clamped = limit.clamp(MIN_CONCURRENT_PROOFS, MAX_CONCURRENT_PROOFS_CEILING);
    MAX_CONCURRENT_PROOFS.store(clamped, Ordering::Release);
    PROVER_GATE.condvar.notify_all();
    clamped
}

/// Bumps the process-wide cancellation epoch. Any batch worker (see `run_prove_batch`) that has
/// not yet begun its individual proof observes the new epoch and abandons that request instead of
/// starting it. Already-running proofs are unaffected -- see the `CANCEL_EPOCH` doc comment.
pub(crate) fn cancel_all() {
    CANCEL_EPOCH.fetch_add(1, Ordering::AcqRel);
}

struct MemoryRegistry {
    params: HashMap<u8, ParamsProver>,
    circuits: HashMap<String, ProvingKeyMaterial>,
}

impl ParamsProverProvider for MemoryRegistry {
    async fn get_params(&self, k: u8) -> io::Result<ParamsProver> {
        self.params
            .get(&k)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("missing params k={k}")))
    }
}

impl Resolver for MemoryRegistry {
    async fn resolve_key(&self, key: KeyLocation) -> io::Result<Option<ProvingKeyMaterial>> {
        Ok(self.circuits.get(key.0.as_ref()).cloned())
    }
}

struct RequestResolver<'a> {
    registry: &'a MemoryRegistry,
    supplied: Option<ProvingKeyMaterial>,
}

impl Resolver for RequestResolver<'_> {
    async fn resolve_key(&self, key: KeyLocation) -> io::Result<Option<ProvingKeyMaterial>> {
        Ok(self
            .supplied
            .clone()
            .or_else(|| self.registry.circuits.get(key.0.as_ref()).cloned()))
    }
}

#[derive(Default)]
struct RegistrySlot {
    generation: u64,
    value: Option<Arc<MemoryRegistry>>,
}

fn registry_slot() -> &'static Mutex<RegistrySlot> {
    REGISTRY.get_or_init(|| Mutex::new(RegistrySlot::default()))
}

fn checked_total(lengths: impl IntoIterator<Item = usize>) -> Result<(), LocalProverError> {
    let total = lengths.into_iter().try_fold(0_usize, |total, length| {
        if length == 0 || length > MAX_ARTIFACT_BYTES {
            return Err(LocalProverError::ResourcePreflightFailed);
        }
        total
            .checked_add(length)
            .ok_or(LocalProverError::ResourcePreflightFailed)
    })?;
    if total > MAX_TOTAL_ARTIFACT_BYTES {
        return Err(LocalProverError::ResourcePreflightFailed);
    }
    Ok(())
}

fn verify_hash(bytes: &[u8], expected: &[u8]) -> Result<(), LocalProverError> {
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    if actual != expected {
        return Err(LocalProverError::IntegrityCheckFailed);
    }
    Ok(())
}

fn validate_location(location: &str) -> Result<(), LocalProverError> {
    if location.is_empty()
        || location.len() > MAX_KEY_LOCATION_BYTES
        || location.chars().any(char::is_control)
    {
        return Err(LocalProverError::InvalidConfiguration);
    }
    Ok(())
}

fn decode_circuit(
    input: &CircuitArtifact<'_>,
) -> Result<(u8, ProvingKeyMaterial), LocalProverError> {
    validate_location(input.key_location)?;
    for (bytes, hash) in [
        (input.prover_key, input.prover_key_sha256),
        (input.verifier_key, input.verifier_key_sha256),
        (input.ir, input.ir_sha256),
    ] {
        verify_hash(bytes, hash)?;
    }
    let ir = IrSource::load_from_tagged(Cursor::new(input.ir))
        .map_err(|_| LocalProverError::InvalidConfiguration)?;
    let prover_key = IrSource::load_prover_key_from_tagged(Cursor::new(input.prover_key))
        .map_err(|_| LocalProverError::InvalidConfiguration)?;
    if prover_key
        .init()
        .map_err(|_| LocalProverError::InvalidConfiguration)?
        .k()
        != ir.k()
    {
        return Err(LocalProverError::InvalidConfiguration);
    }
    let verifier_key: VerifierKey = tagged_deserialize(&mut &input.verifier_key[..])
        .map_err(|_| LocalProverError::InvalidConfiguration)?;
    verifier_key
        .init()
        .map_err(|_| LocalProverError::InvalidConfiguration)?;
    Ok((
        ir.k(),
        ProvingKeyMaterial {
            prover_key: input.prover_key.to_vec(),
            verifier_key: input.verifier_key.to_vec(),
            ir_source: input.ir.to_vec(),
        },
    ))
}

fn build_registry(
    params: &[ParameterArtifact<'_>],
    circuits: &[CircuitArtifact<'_>],
) -> Result<MemoryRegistry, LocalProverError> {
    if params.is_empty() || params.len() > MAX_PARAMETER_COUNT || circuits.len() > MAX_CIRCUIT_COUNT
    {
        return Err(LocalProverError::InvalidConfiguration);
    }
    let lengths = params
        .iter()
        .map(|entry| entry.bytes.len())
        .chain(circuits.iter().flat_map(|entry| {
            [
                entry.prover_key.len(),
                entry.verifier_key.len(),
                entry.ir.len(),
            ]
        }));
    checked_total(lengths)?;
    let mut decoded_params = HashMap::with_capacity(params.len());
    for input in params {
        verify_hash(input.bytes, input.sha256)?;
        let value =
            ParamsProver::read(input.bytes).map_err(|_| LocalProverError::InvalidConfiguration)?;
        if value.0.max_k() != u32::from(input.k) {
            return Err(LocalProverError::InvalidConfiguration);
        }
        if decoded_params.insert(input.k, value).is_some() {
            return Err(LocalProverError::InvalidConfiguration);
        }
    }
    let mut decoded_circuits = HashMap::with_capacity(circuits.len());
    for input in circuits {
        let (k, material) = decode_circuit(input)?;
        if !decoded_params.contains_key(&k)
            || decoded_circuits
                .insert(input.key_location.to_owned(), material)
                .is_some()
        {
            return Err(LocalProverError::InvalidConfiguration);
        }
    }
    Ok(MemoryRegistry {
        params: decoded_params,
        circuits: decoded_circuits,
    })
}

pub(crate) fn configure_registry(
    params: &[ParameterArtifact<'_>],
    circuits: &[CircuitArtifact<'_>],
) -> Result<u64, LocalProverError> {
    let _permit = ExclusiveProverPermit::acquire()?;
    let value = Arc::new(build_registry(params, circuits)?);
    let mut slot = registry_slot()
        .lock()
        .map_err(|_| LocalProverError::NativeInternal)?;
    slot.generation = slot
        .generation
        .checked_add(1)
        .ok_or(LocalProverError::NativeInternal)?;
    if slot.generation == 0 {
        return Err(LocalProverError::NativeInternal);
    }
    slot.value = Some(value);
    Ok(slot.generation)
}

fn configured_registry(handle: u64) -> Result<Arc<MemoryRegistry>, LocalProverError> {
    let slot = registry_slot()
        .lock()
        .map_err(|_| LocalProverError::NativeInternal)?;
    if slot.value.is_none() {
        return Err(LocalProverError::StaleRegistry);
    }
    if handle == 0 || handle != slot.generation {
        return Err(LocalProverError::StaleRegistry);
    }
    slot.value.clone().ok_or(LocalProverError::StaleRegistry)
}

pub(crate) fn close_registry(handle: u64) -> Result<(), LocalProverError> {
    let _permit = ExclusiveProverPermit::acquire()?;
    let mut slot = registry_slot()
        .lock()
        .map_err(|_| LocalProverError::NativeInternal)?;
    if handle == 0 || handle != slot.generation || slot.value.is_none() {
        return Err(LocalProverError::StaleRegistry);
    }
    slot.value = None;
    Ok(())
}

fn request_preflight(request: &[u8]) -> Result<(), LocalProverError> {
    if usize::BITS < 64 || request.is_empty() || request.len() > MAX_REQUEST_BYTES {
        return Err(LocalProverError::ResourcePreflightFailed);
    }
    Ok(())
}

fn prover_pool() -> Result<&'static ThreadPool, LocalProverError> {
    PROVER_POOL
        .get_or_init(|| {
            ThreadPoolBuilder::new()
                .num_threads(PROVER_THREAD_COUNT)
                .build()
        })
        .as_ref()
        .map_err(|_| LocalProverError::ResourcePreflightFailed)
}

fn serialize_response<T: midnight_serialize::Serializable + midnight_serialize::Tagged>(
    value: &T,
) -> Result<Vec<u8>, LocalProverError> {
    let mut response = Vec::new();
    tagged_serialize(value, &mut response).map_err(|_| LocalProverError::NativeInternal)?;
    Ok(response)
}

pub(crate) fn run_check(handle: u64, request: &[u8]) -> Result<Vec<u8>, LocalProverError> {
    request_preflight(request)?;
    let _permit = SharedProverPermit::acquire()?;
    let registry = configured_registry(handle)?;
    let (versioned, supplied_ir): (ProofPreimageVersioned, Option<WrappedIr>) =
        tagged_deserialize(&mut &request[..]).map_err(|_| LocalProverError::InvalidRequest)?;
    let preimage = match versioned {
        ProofPreimageVersioned::V2(preimage) => preimage,
        _ => return Err(LocalProverError::InvalidRequest),
    };
    let ir_bytes = supplied_ir.map(|wrapped| wrapped.0).or_else(|| {
        registry
            .circuits
            .get(preimage.key_location.0.as_ref())
            .map(|material| material.ir_source.clone())
    });
    let ir_bytes = ir_bytes.ok_or(LocalProverError::UnsupportedCircuit)?;
    if ir_bytes.is_empty() || ir_bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(LocalProverError::ResourcePreflightFailed);
    }
    let ir = IrSource::load_from_tagged(Cursor::new(ir_bytes))
        .map_err(|_| LocalProverError::UnsupportedCircuit)?;
    let result = preimage
        .check(&ir)
        .map_err(|_| LocalProverError::CheckFailed)?
        .into_iter()
        .map(|entry| entry.and_then(|value| u64::try_from(value).ok()))
        .collect::<Vec<_>>();
    serialize_response(&result)
}

fn validate_supplied_material(material: &ProvingKeyMaterial) -> Result<(), LocalProverError> {
    checked_total([
        material.prover_key.len(),
        material.verifier_key.len(),
        material.ir_source.len(),
    ])?;
    IrSource::load_from_tagged(Cursor::new(&material.ir_source))
        .map_err(|_| LocalProverError::UnsupportedCircuit)?;
    Ok(())
}

pub(crate) fn run_prove(handle: u64, request: &[u8]) -> Result<Vec<u8>, LocalProverError> {
    request_preflight(request)?;
    let _permit = SharedProverPermit::acquire()?;
    let registry = configured_registry(handle)?;
    let profiling = PROFILE_STAGES.load(Ordering::Acquire);
    let request_bytes = request.len();

    let deserialize_start = Instant::now();
    let (versioned, supplied, binding_input): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &request[..]).map_err(|_| LocalProverError::InvalidRequest)?;
    let deserialize_request_micros = stage_micros(deserialize_start);

    let mut preimage = match versioned {
        ProofPreimageVersioned::V2(preimage) => preimage,
        _ => return Err(LocalProverError::InvalidRequest),
    };
    if let Some(binding_input) = binding_input {
        let mut inner = (*preimage).clone();
        inner.binding_input = binding_input;
        preimage = Arc::new(inner);
    }

    let select_start = Instant::now();
    let selected = supplied
        .as_ref()
        .or_else(|| registry.circuits.get(preimage.key_location.0.as_ref()));
    let selected_material = selected.ok_or(LocalProverError::UnsupportedCircuit)?;
    validate_supplied_material(selected_material)?;
    let select_material_micros = stage_micros(select_start);
    let key_location = preimage.key_location.0.to_string();
    let cached_material = profiling.then(|| selected_material.clone());

    let resolver = RequestResolver {
        registry: &registry,
        supplied,
    };
    let prove_start = Instant::now();
    let (proof, _) = prover_pool()?
        .install(|| {
            futures_executor::block_on(preimage.prove::<IrSource>(OsRng, &*registry, &resolver))
        })
        .map_err(|_| LocalProverError::ProofFailed)?;
    let prove_call_micros = stage_micros(prove_start);

    let serialize_start = Instant::now();
    let response = serialize_response(&ProofVersioned::V2(proof))?;
    let serialize_response_micros = stage_micros(serialize_start);

    maybe_record_prove_timing(
        cached_material,
        key_location,
        request_bytes,
        ProveStageDurations {
            deserialize_request_micros,
            select_material_micros,
            prove_call_micros,
            serialize_response_micros,
        },
    );

    Ok(response)
}

/// The exact check a batch worker performs before starting its individual proof: has
/// `CANCEL_EPOCH` moved past the epoch captured when the batch began? Factored out so tests can
/// exercise the real predicate rather than a re-implementation of it.
fn worker_should_abandon(epoch_at_start: u64) -> bool {
    CANCEL_EPOCH.load(Ordering::Acquire) != epoch_at_start
}

/// Fans out up to `crate::transaction::MAX_PROOF_BATCH` prove requests across OS threads, each
/// calling `run_prove` so it acquires its own `SharedProverPermit` and installs into the shared
/// Rayon pool -- `MAX_CONCURRENT_PROOFS` still does the admission and Rayon work-stealing still
/// balances the CPU, exactly as for a single `run_prove` call. All-or-nothing: the first error in
/// request order (not completion order, so the outcome is deterministic regardless of thread
/// timing) is returned and no partial response set is ever produced by this function.
///
/// Every request is copied into a `Zeroizing<Vec<u8>>` up front. That copy -- the batch container
/// this function owns -- is wiped by `Zeroizing`'s `Drop` when `owned_requests` goes out of scope,
/// which happens whether this function returns `Ok`, returns an `Err` from a real proof failure,
/// or abandons a request because `CANCEL_EPOCH` moved past `epoch_at_start` before that request's
/// worker started; there is no path out of this function that skips it.
pub(crate) fn run_prove_batch(
    handle: u64,
    requests: &[&[u8]],
) -> Result<Vec<Vec<u8>>, LocalProverError> {
    let epoch_at_start = CANCEL_EPOCH.load(Ordering::Acquire);
    let owned_requests: Vec<Zeroizing<Vec<u8>>> = requests
        .iter()
        .map(|request| Zeroizing::new((*request).to_vec()))
        .collect();

    let results: Vec<Result<Vec<u8>, LocalProverError>> = std::thread::scope(|scope| {
        let workers: Vec<_> = owned_requests
            .iter()
            .map(|request| {
                scope.spawn(move || {
                    if worker_should_abandon(epoch_at_start) {
                        // Reuses PROVER_BUSY rather than adding a new FFI error code: from the
                        // caller's perspective this request was never admitted, exactly like the
                        // existing over-capacity rejection, and a new code would change the stable
                        // 1..=10 mapping Swift/Kotlin decode by index.
                        return Err(LocalProverError::ProverBusy);
                    }
                    run_prove(handle, request)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or(Err(LocalProverError::NativeInternal))
            })
            .collect()
    });

    results.into_iter().collect()
}

fn deterministic_zswap_spend_preimage()
-> Result<Arc<midnight_transient_crypto::proofs::ProofPreimage>, LocalProverError> {
    let mut rng = StdRng::seed_from_u64(0x42);
    let qualified_coin = QualifiedCoinInfo {
        value: Default::default(),
        type_: Default::default(),
        nonce: rand::Rng::r#gen(&mut rng),
        mt_index: 0,
    };
    let coin = CoinInfo::from(&qualified_coin);
    let recipient = midnight_coin_structure::transfer::Recipient::Contract(Default::default());
    let tree = MerkleTree::<(), InMemoryDB>::blank(32)
        .try_update_hash(0, coin.commitment(&recipient).0, ())
        .map_err(|_| LocalProverError::ProofFailed)?
        .rehash();
    Input::new_contract_owned(&mut rng, &qualified_coin, None, Default::default(), &tree)
        .map(|input| input.proof)
        .map_err(|_| LocalProverError::ProofFailed)
}

fn deterministic_zswap_output_preimage()
-> Result<Arc<midnight_transient_crypto::proofs::ProofPreimage>, LocalProverError> {
    let mut rng = StdRng::seed_from_u64(0x42);
    let qualified_coin = QualifiedCoinInfo {
        value: Default::default(),
        type_: Default::default(),
        nonce: rand::Rng::r#gen(&mut rng),
        mt_index: 0,
    };
    let coin = CoinInfo::from(&qualified_coin);
    Output::<_, InMemoryDB>::new_contract_owned(&mut rng, &coin, None, Default::default())
        .map(|output| output.proof)
        .map_err(|_| LocalProverError::ProofFailed)
}

pub fn deterministic_zswap_spend_request() -> Result<Vec<u8>, LocalProverError> {
    serialize_response(&(
        ProofPreimageVersioned::V2(deterministic_zswap_spend_preimage()?),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    ))
}

pub fn deterministic_zswap_output_request() -> Result<Vec<u8>, LocalProverError> {
    serialize_response(&(
        ProofPreimageVersioned::V2(deterministic_zswap_output_preimage()?),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    ))
}

pub fn deterministic_zswap_spend_check_request() -> Result<Vec<u8>, LocalProverError> {
    serialize_response(&(
        ProofPreimageVersioned::V2(deterministic_zswap_spend_preimage()?),
        None::<WrappedIr>,
    ))
}

#[cfg(test)]
mod tests;
