use std::collections::HashMap;
use std::io::{self, Cursor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

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

mod ffi;

const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const MAX_ARTIFACT_BYTES: usize = 512 * 1024 * 1024;
const MAX_TOTAL_ARTIFACT_BYTES: usize = 2 * 1024 * 1024 * 1024;
const MAX_PARAMETER_COUNT: usize = 32;
const MAX_CIRCUIT_COUNT: usize = 256;
const MAX_KEY_LOCATION_BYTES: usize = 1_024;
// Four threads match the measured Android device's performance-core cluster
// while keeping mobile proof parallelism bounded for memory and thermals.
const PROVER_THREAD_COUNT: usize = 4;

static PROVER_ACTIVE: AtomicBool = AtomicBool::new(false);
static PROVER_POOL: OnceLock<Result<ThreadPool, rayon::ThreadPoolBuildError>> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<RegistrySlot>> = OnceLock::new();
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

struct ProverPermit;

impl ProverPermit {
    fn acquire() -> Result<Self, LocalProverError> {
        PROVER_ACTIVE
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| Self)
            .map_err(|_| LocalProverError::ProverBusy)
    }
}

impl Drop for ProverPermit {
    fn drop(&mut self) {
        PROVER_ACTIVE.store(false, Ordering::Release);
    }
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
    let _permit = ProverPermit::acquire()?;
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
    let _permit = ProverPermit::acquire()?;
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
    let _permit = ProverPermit::acquire()?;
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
    let _permit = ProverPermit::acquire()?;
    let registry = configured_registry(handle)?;
    let (versioned, supplied, binding_input): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &request[..]).map_err(|_| LocalProverError::InvalidRequest)?;
    let mut preimage = match versioned {
        ProofPreimageVersioned::V2(preimage) => preimage,
        _ => return Err(LocalProverError::InvalidRequest),
    };
    if let Some(binding_input) = binding_input {
        let mut inner = (*preimage).clone();
        inner.binding_input = binding_input;
        preimage = Arc::new(inner);
    }
    let selected = supplied
        .as_ref()
        .or_else(|| registry.circuits.get(preimage.key_location.0.as_ref()));
    validate_supplied_material(selected.ok_or(LocalProverError::UnsupportedCircuit)?)?;
    let resolver = RequestResolver {
        registry: &registry,
        supplied,
    };
    let (proof, _) = prover_pool()?
        .install(|| {
            futures_executor::block_on(preimage.prove::<IrSource>(OsRng, &*registry, &resolver))
        })
        .map_err(|_| LocalProverError::ProofFailed)?;
    serialize_response(&ProofVersioned::V2(proof))
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
