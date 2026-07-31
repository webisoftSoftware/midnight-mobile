use std::sync::Arc;

use midnight_ledger::structure::ProofVersioned;
use midnight_serialize::tagged_deserialize;
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::proofs::{
    KeyLocation, ProofPreimage, ProvingKeyMaterial, WrappedIr,
};

use super::*;

fn synthetic_preimage(location: &'static str) -> Arc<ProofPreimage> {
    Arc::new(ProofPreimage {
        inputs: vec![1_u64.into()],
        private_transcript: vec![],
        public_transcript_inputs: vec![],
        public_transcript_outputs: vec![],
        binding_input: 0_u64.into(),
        communications_commitment: None,
        key_location: KeyLocation(std::borrow::Cow::Borrowed(location)),
    })
}

fn serialize<T: midnight_serialize::Serializable + midnight_serialize::Tagged>(
    value: &T,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    tagged_serialize(value, &mut bytes).unwrap();
    bytes
}

fn install_synthetic_registry() -> u64 {
    let mut slot = registry_slot().lock().unwrap();
    slot.generation = slot.generation.saturating_add(1).max(1);
    slot.value = Some(Arc::new(MemoryRegistry {
        params: HashMap::new(),
        circuits: HashMap::new(),
    }));
    slot.generation
}

#[test]
fn official_request_generators_have_exact_tuple_shapes() {
    let prove = deterministic_zswap_spend_request().unwrap();
    let (versioned, material, binding): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &prove[..]).unwrap();
    assert!(matches!(versioned, ProofPreimageVersioned::V2(_)));
    assert!(material.is_none());
    assert!(binding.is_none());

    let check = deterministic_zswap_spend_check_request().unwrap();
    let (versioned, ir): (ProofPreimageVersioned, Option<WrappedIr>) =
        tagged_deserialize(&mut &check[..]).unwrap();
    assert!(matches!(versioned, ProofPreimageVersioned::V2(_)));
    assert!(ir.is_none());
}

#[test]
fn hash_location_count_and_size_preflights_are_typed() {
    let hash = Sha256::digest(b"artifact");
    assert!(verify_hash(b"artifact", hash.as_ref()).is_ok());
    assert_eq!(
        verify_hash(b"artifact", &[0; 32]),
        Err(LocalProverError::IntegrityCheckFailed)
    );
    assert!(validate_location("midnight/custom/circuit").is_ok());
    assert_eq!(
        validate_location("bad\nlocation"),
        Err(LocalProverError::InvalidConfiguration)
    );
    assert_eq!(
        checked_total([0_usize]),
        Err(LocalProverError::ResourcePreflightFailed)
    );
    assert_eq!(
        build_registry(&[], &[]).map(|_| ()),
        Err(LocalProverError::InvalidConfiguration)
    );
}

#[test]
fn malformed_missing_and_inline_invalid_requests_are_typed() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    assert_eq!(
        run_check(handle, b"bad"),
        Err(LocalProverError::InvalidRequest)
    );
    assert_eq!(
        run_prove(handle, b"bad"),
        Err(LocalProverError::InvalidRequest)
    );
    let check = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("missing/circuit")),
        None::<WrappedIr>,
    ));
    assert_eq!(
        run_check(handle, &check),
        Err(LocalProverError::UnsupportedCircuit)
    );
    let inline = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(vec![1, 2, 3])),
    ));
    assert_eq!(
        run_check(handle, &inline),
        Err(LocalProverError::UnsupportedCircuit)
    );
}

#[test]
fn registry_generations_close_and_stale_handles_fail_closed() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    assert!(configured_registry(handle).is_ok());
    assert_eq!(
        configured_registry(handle.saturating_add(1)).map(|_| ()),
        Err(LocalProverError::StaleRegistry)
    );
    assert!(close_registry(handle).is_ok());
    assert_eq!(close_registry(handle), Err(LocalProverError::StaleRegistry));
}

#[test]
fn a_concurrent_operation_receives_busy() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let holder_barrier = barrier.clone();
    let holder = std::thread::spawn(move || {
        let _permit = ProverPermit::acquire().unwrap();
        holder_barrier.wait();
        release_receiver.recv().unwrap();
    });
    barrier.wait();
    assert_eq!(
        ProverPermit::acquire().map(|_| ()),
        Err(LocalProverError::ProverBusy)
    );
    release_sender.send(()).unwrap();
    holder.join().unwrap();
}

#[test]
#[ignore = "requires staged public artifacts"]
fn staged_artifacts_produce_and_check_official_responses() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let directory = std::path::PathBuf::from(
        std::env::var_os("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR")
            .expect("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR must be set"),
    );
    let read = |name: &str| std::fs::read(directory.join(name)).unwrap();
    let params = read("bls_midnight_2p15");
    let prover = read("zswap/9/spend.prover");
    let verifier = read("zswap/9/spend.verifier");
    let ir = read("zswap/9/spend.bzkir");
    let digest = |bytes: &[u8]| Sha256::digest(bytes).to_vec();
    let params_hash = digest(&params);
    let prover_hash = digest(&prover);
    let verifier_hash = digest(&verifier);
    let ir_hash = digest(&ir);
    let handle = configure_registry(
        &[ParameterArtifact {
            k: 15,
            bytes: &params,
            sha256: &params_hash,
        }],
        &[CircuitArtifact {
            key_location: "midnight/zswap/spend",
            prover_key: &prover,
            prover_key_sha256: &prover_hash,
            verifier_key: &verifier,
            verifier_key_sha256: &verifier_hash,
            ir: &ir,
            ir_sha256: &ir_hash,
        }],
    )
    .unwrap();
    let check = run_check(handle, &deterministic_zswap_spend_check_request().unwrap()).unwrap();
    let _: Vec<Option<u64>> = tagged_deserialize(&mut &check[..]).unwrap();
    let proof = run_prove(handle, &deterministic_zswap_spend_request().unwrap()).unwrap();
    let decoded: ProofVersioned = tagged_deserialize(&mut &proof[..]).unwrap();
    assert!(matches!(decoded, ProofVersioned::V2(_)));
    close_registry(handle).unwrap();
}
