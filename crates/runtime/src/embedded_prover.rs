use std::io::{self, Cursor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use midnight_coin_structure::coin::{Info as CoinInfo, QualifiedInfo as QualifiedCoinInfo};
use midnight_ledger::structure::{ProofPreimageVersioned, ProofVersioned};
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use midnight_storage::db::InMemoryDB;
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::merkle_tree::MerkleTree;
use midnight_transient_crypto::proofs::{
    KeyLocation, ParamsProver, ParamsProverProvider, ProvingKeyMaterial, Resolver, VerifierKey,
    Zkir,
};
use midnight_zkir::IrSource;
use midnight_zswap::Input;
use rand::SeedableRng;
use rand::rngs::{OsRng, StdRng};
use rayon::{ThreadPool, ThreadPoolBuilder};
use sha2::{Digest, Sha256};

mod ffi;

const SPEND_KEY_LOCATION: &str = "midnight/zswap/spend";
const EXPECTED_K: u8 = 15;
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const PARAMS_BYTES: usize = 6_291_844;
const PROVER_KEY_BYTES: usize = 11_020_001;
const VERIFIER_KEY_BYTES: usize = 2_311;
const IR_BYTES: usize = 1_294;
const PARAMS_HASH: &str = "724c7c3d779148bb113c7ee9c034b2f27db16e6bdf315fde90105a9bad00b1de";
const PROVER_KEY_HASH: &str = "19d234b5c68b7212ad6b0ec9334a95594748154128f3704eb576bcc843cc5c45";
const VERIFIER_KEY_HASH: &str = "544554effd7ae9fb9063be52a9ec2a986756301071fcd97bb4598fb45a335658";
const IR_HASH: &str = "7cb5bbcf67cb212a3336fb439a77e8f32f0aa8a56185c8e1247d6cbfc7300205";

static PROVER_ACTIVE: AtomicBool = AtomicBool::new(false);
static PROVER_POOL: OnceLock<Result<ThreadPool, rayon::ThreadPoolBuildError>> = OnceLock::new();
#[cfg(test)]
static PROVER_TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EmbeddedProofResult {
    pub tagged_proof_bytes: Vec<u8>,
    pub artifact_decoding_millis: u64,
    pub proving_and_self_verification_millis: u64,
    pub proof_size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum EmbeddedProverError {
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
}

struct ProverPermit;

impl ProverPermit {
    fn acquire() -> Result<Self, EmbeddedProverError> {
        PROVER_ACTIVE
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| Self)
            .map_err(|_| EmbeddedProverError::ProverBusy)
    }
}

impl Drop for ProverPermit {
    fn drop(&mut self) {
        PROVER_ACTIVE.store(false, Ordering::Release);
    }
}

struct MemoryArtifacts {
    params: ParamsProver,
    material: Mutex<Option<ProvingKeyMaterial>>,
}

fn validate_params_k(k: u8) -> io::Result<()> {
    if k != EXPECTED_K {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only k=15 parameters are staged",
        ));
    }
    Ok(())
}

fn resolve_memory_key(
    material: &Mutex<Option<ProvingKeyMaterial>>,
    key: KeyLocation,
) -> io::Result<Option<ProvingKeyMaterial>> {
    if key.0 != SPEND_KEY_LOCATION {
        return Ok(None);
    }
    material
        .lock()
        .map_err(|_| io::Error::other("artifact resolver is poisoned"))
        .map(|mut material| material.take())
}

impl ParamsProverProvider for MemoryArtifacts {
    async fn get_params(&self, k: u8) -> io::Result<ParamsProver> {
        validate_params_k(k)?;
        Ok(self.params.clone())
    }
}

impl Resolver for MemoryArtifacts {
    async fn resolve_key(&self, key: KeyLocation) -> io::Result<Option<ProvingKeyMaterial>> {
        resolve_memory_key(&self.material, key)
    }
}

struct DecodedRequest {
    preimage: Arc<midnight_transient_crypto::proofs::ProofPreimage>,
}

fn checked_elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn request_preflight(request: &[u8]) -> Result<(), EmbeddedProverError> {
    if usize::BITS < 64 || request.len() > MAX_REQUEST_BYTES {
        return Err(EmbeddedProverError::ResourcePreflightFailed);
    }
    Ok(())
}

fn artifact_preflight(artifacts: [&[u8]; 4]) -> Result<(), EmbeddedProverError> {
    if artifacts.iter().any(|artifact| artifact.is_empty()) {
        return Err(EmbeddedProverError::ResourcePreflightFailed);
    }
    if artifacts.map(<[u8]>::len) != [PARAMS_BYTES, PROVER_KEY_BYTES, VERIFIER_KEY_BYTES, IR_BYTES]
    {
        return Err(EmbeddedProverError::IntegrityCheckFailed);
    }
    Ok(())
}

fn decode_request(request: &[u8]) -> Result<DecodedRequest, EmbeddedProverError> {
    let (versioned, supplied_material, binding_input): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &request[..]).map_err(|_| EmbeddedProverError::InvalidRequest)?;
    if supplied_material.is_some() {
        return Err(EmbeddedProverError::InvalidRequest);
    }
    let mut preimage = match versioned {
        ProofPreimageVersioned::V2(preimage) => preimage,
        _ => return Err(EmbeddedProverError::InvalidRequest),
    };
    if preimage.key_location.0 != SPEND_KEY_LOCATION {
        return Err(EmbeddedProverError::UnsupportedCircuit);
    }
    if let Some(binding_input) = binding_input {
        let mut inner = (*preimage).clone();
        inner.binding_input = binding_input;
        preimage = Arc::new(inner);
    }
    Ok(DecodedRequest { preimage })
}

fn check_hash(bytes: &[u8], expected: &str) -> Result<(), EmbeddedProverError> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual == expected {
        Ok(())
    } else {
        Err(EmbeddedProverError::IntegrityCheckFailed)
    }
}

fn validate_artifact_hashes(
    artifacts: [&[u8]; 4],
    expected_hashes: [&str; 4],
) -> Result<(), EmbeddedProverError> {
    artifacts
        .into_iter()
        .zip(expected_hashes)
        .try_for_each(|(bytes, expected)| check_hash(bytes, expected))
}

fn decode_artifacts(
    params_k15: &[u8],
    prover_key: &[u8],
    verifier_key: &[u8],
    ir: &[u8],
) -> Result<MemoryArtifacts, EmbeddedProverError> {
    validate_artifact_hashes(
        [params_k15, prover_key, verifier_key, ir],
        [PARAMS_HASH, PROVER_KEY_HASH, VERIFIER_KEY_HASH, IR_HASH],
    )?;
    let parsed_ir = IrSource::load_from_tagged(Cursor::new(ir))
        .map_err(|_| EmbeddedProverError::IntegrityCheckFailed)?;
    if parsed_ir.k() != EXPECTED_K {
        return Err(EmbeddedProverError::UnsupportedCircuit);
    }
    let parsed_prover_key = IrSource::load_prover_key_from_tagged(Cursor::new(prover_key))
        .map_err(|_| EmbeddedProverError::IntegrityCheckFailed)?;
    parsed_prover_key
        .init()
        .map_err(|_| EmbeddedProverError::IntegrityCheckFailed)?;
    let parsed_verifier_key: VerifierKey = tagged_deserialize(&mut &verifier_key[..])
        .map_err(|_| EmbeddedProverError::IntegrityCheckFailed)?;
    parsed_verifier_key
        .init()
        .map_err(|_| EmbeddedProverError::IntegrityCheckFailed)?;
    let params =
        ParamsProver::read(params_k15).map_err(|_| EmbeddedProverError::IntegrityCheckFailed)?;
    Ok(MemoryArtifacts {
        params,
        material: Mutex::new(Some(ProvingKeyMaterial {
            prover_key: prover_key.to_vec(),
            verifier_key: verifier_key.to_vec(),
            ir_source: ir.to_vec(),
        })),
    })
}

fn prover_pool() -> Result<&'static ThreadPool, EmbeddedProverError> {
    PROVER_POOL
        .get_or_init(|| ThreadPoolBuilder::new().num_threads(2).build())
        .as_ref()
        .map_err(|_| EmbeddedProverError::ResourcePreflightFailed)
}

pub fn run_embedded_prover_probe(
    request: &[u8],
    params_k15: &[u8],
    prover_key: &[u8],
    verifier_key: &[u8],
    ir: &[u8],
) -> Result<EmbeddedProofResult, EmbeddedProverError> {
    request_preflight(request)?;
    let _permit = ProverPermit::acquire()?;
    let decoded_request = decode_request(request)?;
    artifact_preflight([params_k15, prover_key, verifier_key, ir])?;
    let decode_started = Instant::now();
    let artifacts = decode_artifacts(params_k15, prover_key, verifier_key, ir)?;
    let artifact_decoding_millis = checked_elapsed_millis(decode_started);
    let proving_started = Instant::now();
    let (proof, _) = prover_pool()?
        .install(|| {
            futures_executor::block_on(
                decoded_request
                    .preimage
                    .prove::<IrSource>(OsRng, &artifacts, &artifacts),
            )
        })
        .map_err(|_| EmbeddedProverError::ProofFailed)?;
    let proving_and_self_verification_millis = checked_elapsed_millis(proving_started);
    let mut tagged_proof_bytes = Vec::new();
    tagged_serialize(&ProofVersioned::V2(proof), &mut tagged_proof_bytes)
        .map_err(|_| EmbeddedProverError::ProofFailed)?;
    let proof_size =
        u64::try_from(tagged_proof_bytes.len()).map_err(|_| EmbeddedProverError::ProofFailed)?;
    Ok(EmbeddedProofResult {
        tagged_proof_bytes,
        artifact_decoding_millis,
        proving_and_self_verification_millis,
        proof_size,
    })
}

#[uniffi::export]
pub fn run_embedded_prover_probe_owned(
    request: Vec<u8>,
    params_k15: Vec<u8>,
    prover_key: Vec<u8>,
    verifier_key: Vec<u8>,
    ir: Vec<u8>,
) -> Result<EmbeddedProofResult, EmbeddedProverError> {
    run_embedded_prover_probe(&request, &params_k15, &prover_key, &verifier_key, &ir)
}

pub fn deterministic_zswap_spend_request() -> Result<Vec<u8>, EmbeddedProverError> {
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
        .map_err(|_| EmbeddedProverError::ProofFailed)?
        .rehash();
    let input =
        Input::new_contract_owned(&mut rng, &qualified_coin, None, Default::default(), &tree)
            .map_err(|_| EmbeddedProverError::ProofFailed)?;
    let payload = (
        ProofPreimageVersioned::V2(input.proof),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    );
    let mut request = Vec::new();
    tagged_serialize(&payload, &mut request).map_err(|_| EmbeddedProverError::ProofFailed)?;
    Ok(request)
}

#[cfg(test)]
mod tests;
