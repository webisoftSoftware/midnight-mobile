use std::sync::Arc;

use midnight_ledger::structure::ProofVersioned;
use midnight_serialize::tagged_deserialize;
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::proofs::{KeyLocation, ProofPreimage, ProvingKeyMaterial};

use super::*;

fn request_with_location(location: &'static str) -> Vec<u8> {
    let preimage = ProofPreimage {
        inputs: vec![1_u64.into()],
        private_transcript: vec![],
        public_transcript_inputs: vec![],
        public_transcript_outputs: vec![],
        binding_input: 0_u64.into(),
        communications_commitment: None,
        key_location: KeyLocation(std::borrow::Cow::Borrowed(location)),
    };
    let payload = (
        ProofPreimageVersioned::V2(Arc::new(preimage)),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    );
    let mut request = Vec::new();
    tagged_serialize(&payload, &mut request).unwrap();
    request
}

fn request_with_options(material: Option<ProvingKeyMaterial>, binding: Option<Fr>) -> Vec<u8> {
    let preimage = ProofPreimage {
        inputs: vec![1_u64.into()],
        private_transcript: vec![],
        public_transcript_inputs: vec![],
        public_transcript_outputs: vec![],
        binding_input: 0_u64.into(),
        communications_commitment: None,
        key_location: KeyLocation(std::borrow::Cow::Borrowed(SPEND_KEY_LOCATION)),
    };
    let payload = (
        ProofPreimageVersioned::V2(Arc::new(preimage)),
        material,
        binding,
    );
    let mut request = Vec::new();
    tagged_serialize(&payload, &mut request).unwrap();
    request
}

#[test]
fn deterministic_request_is_official_prove_shape() {
    let request = deterministic_zswap_spend_request().unwrap();
    let (versioned, material, binding): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &request[..]).unwrap();
    assert!(material.is_none());
    assert!(binding.is_none());
    assert!(matches!(&versioned, ProofPreimageVersioned::V2(_)));
    if let ProofPreimageVersioned::V2(preimage) = versioned {
        assert_eq!(preimage.key_location.0, SPEND_KEY_LOCATION);
    }
}

#[test]
fn malformed_and_unsupported_requests_are_typed() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let bytes = [1_u8];
    assert_eq!(
        run_embedded_prover_probe(&[], &bytes, &bytes, &bytes, &bytes),
        Err(EmbeddedProverError::InvalidRequest)
    );
    assert_eq!(
        run_embedded_prover_probe(b"bad", &bytes, &bytes, &bytes, &bytes),
        Err(EmbeddedProverError::InvalidRequest)
    );
    assert_eq!(
        run_embedded_prover_probe(
            &request_with_location("external/circuit"),
            &bytes,
            &bytes,
            &bytes,
            &bytes,
        ),
        Err(EmbeddedProverError::UnsupportedCircuit)
    );
}

#[test]
fn supplied_material_is_rejected_and_binding_overrides_are_decoded() {
    let supplied_material = ProvingKeyMaterial {
        prover_key: vec![],
        verifier_key: vec![],
        ir_source: vec![],
    };
    assert_eq!(
        decode_request(&request_with_options(Some(supplied_material), None)).map(|_| ()),
        Err(EmbeddedProverError::InvalidRequest)
    );

    let binding = Fr::from(17_u64);
    let decoded = decode_request(&request_with_options(None, Some(binding))).unwrap();
    assert_eq!(decoded.preimage.binding_input, binding);
}

#[test]
fn missing_and_corrupt_artifacts_are_typed() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let request = deterministic_zswap_spend_request().unwrap();
    assert_eq!(
        run_embedded_prover_probe(&request, &[], &[1], &[1], &[1]),
        Err(EmbeddedProverError::ResourcePreflightFailed)
    );
    let params = vec![0; PARAMS_BYTES];
    let prover = vec![0; PROVER_KEY_BYTES];
    let verifier = vec![0; VERIFIER_KEY_BYTES];
    let ir = vec![0; IR_BYTES];
    assert_eq!(
        run_embedded_prover_probe(&request, &params, &prover, &verifier, &ir),
        Err(EmbeddedProverError::IntegrityCheckFailed)
    );
    assert_eq!(
        artifact_preflight([&[1], &[1], &[1], &[1]]),
        Err(EmbeddedProverError::IntegrityCheckFailed)
    );
}

#[test]
fn a_concurrent_request_receives_busy() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let holder_barrier = barrier.clone();
    let holder = std::thread::spawn(move || {
        let _permit = loop {
            match ProverPermit::acquire() {
                Ok(permit) => break permit,
                Err(EmbeddedProverError::ProverBusy) => std::thread::yield_now(),
                Err(error) => assert_eq!(error, EmbeddedProverError::ProverBusy),
            }
        };
        holder_barrier.wait();
        release_receiver.recv().unwrap();
    });
    barrier.wait();
    let bytes = [1_u8];
    assert_eq!(
        run_embedded_prover_probe(b"bad", &bytes, &bytes, &bytes, &bytes),
        Err(EmbeddedProverError::ProverBusy)
    );
    release_sender.send(()).unwrap();
    holder.join().unwrap();
}

#[test]
fn helper_paths_cover_hash_pool_and_elapsed_behavior() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    assert!(check_hash(b"fixture", &hex::encode(Sha256::digest(b"fixture"))).is_ok());
    assert_eq!(
        check_hash(b"fixture", &"00".repeat(32)),
        Err(EmbeddedProverError::IntegrityCheckFailed)
    );
    assert!(prover_pool().is_ok());
    assert!(checked_elapsed_millis(Instant::now()) < 1_000);
    assert_eq!(
        run_embedded_prover_probe_owned(vec![], vec![1], vec![1], vec![1], vec![1]),
        Err(EmbeddedProverError::InvalidRequest)
    );
}

#[test]
fn in_memory_provider_validation_and_resolution_are_typed() {
    assert!(validate_params_k(EXPECTED_K).is_ok());
    assert_eq!(
        validate_params_k(EXPECTED_K - 1).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );

    let material = Mutex::new(Some(ProvingKeyMaterial {
        prover_key: vec![1],
        verifier_key: vec![2],
        ir_source: vec![3],
    }));
    assert!(
        resolve_memory_key(
            &material,
            KeyLocation(std::borrow::Cow::Borrowed("external/circuit"))
        )
        .unwrap()
        .is_none()
    );
    assert!(
        resolve_memory_key(
            &material,
            KeyLocation(std::borrow::Cow::Borrowed(SPEND_KEY_LOCATION))
        )
        .unwrap()
        .is_some()
    );
    assert!(
        resolve_memory_key(
            &material,
            KeyLocation(std::borrow::Cow::Borrowed(SPEND_KEY_LOCATION))
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn all_artifact_hashes_are_validated() {
    let artifacts: [&[u8]; 4] = [b"params", b"prover", b"verifier", b"ir"];
    let hashes = artifacts.map(|bytes| hex::encode(Sha256::digest(bytes)));
    assert!(
        validate_artifact_hashes(
            artifacts,
            [
                hashes[0].as_str(),
                hashes[1].as_str(),
                hashes[2].as_str(),
                hashes[3].as_str(),
            ],
        )
        .is_ok()
    );
}

#[test]
#[ignore = "requires staged public artifacts"]
fn staged_artifacts_produce_a_tagged_v2_proof() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let directory = std::path::PathBuf::from(
        std::env::var_os("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR")
            .expect("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR must name the staged artifact directory"),
    );
    let read = |name: &str| std::fs::read(directory.join(name)).unwrap();
    let result = run_embedded_prover_probe(
        &deterministic_zswap_spend_request().unwrap(),
        &read("bls_midnight_2p15"),
        &read("zswap/9/spend.prover"),
        &read("zswap/9/spend.verifier"),
        &read("zswap/9/spend.bzkir"),
    )
    .unwrap();
    assert_eq!(result.proof_size as usize, result.tagged_proof_bytes.len());
    let decoded: ProofVersioned = tagged_deserialize(&mut &result.tagged_proof_bytes[..]).unwrap();
    assert!(matches!(decoded, ProofVersioned::V2(_)));
}
