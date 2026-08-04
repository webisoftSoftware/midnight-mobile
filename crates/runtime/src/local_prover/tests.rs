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

fn minimal_ir_bytes(num_inputs: u32) -> Vec<u8> {
    let ir = IrSource {
        version: Default::default(),
        num_inputs,
        do_communications_commitment: false,
        instructions: Arc::new(vec![]),
    };
    let mut bytes = Vec::new();
    ir.serialize_to_tagged(&mut bytes).unwrap();
    bytes
}

fn material(ir_source: Vec<u8>) -> ProvingKeyMaterial {
    ProvingKeyMaterial {
        prover_key: vec![1],
        verifier_key: vec![2],
        ir_source,
    }
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
fn official_request_generators_have_exact_tuple_shapes() -> Result<(), &'static str> {
    let prove = deterministic_zswap_spend_request().unwrap();
    let (versioned, material, binding): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &prove[..]).unwrap();
    assert!(matches!(versioned, ProofPreimageVersioned::V2(_)));
    assert!(material.is_none());
    assert!(binding.is_none());

    let output = deterministic_zswap_output_request().unwrap();
    let (versioned, material, binding): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &output[..]).unwrap();
    let ProofPreimageVersioned::V2(output_preimage) = versioned else {
        return Err("deterministic output request must use V2");
    };
    assert_eq!(output_preimage.key_location.0, "midnight/zswap/output");
    assert!(material.is_none());
    assert!(binding.is_none());
    assert_eq!(output, deterministic_zswap_output_request().unwrap());
    assert_eq!(output.len(), 430);
    assert_eq!(
        hex::encode(Sha256::digest(&output)),
        "af7c9cb545b8601be70c6a9cfe39bd06d32083f9fbde36c5f84500c25b9c0172"
    );

    let check = deterministic_zswap_spend_check_request().unwrap();
    let (versioned, ir): (ProofPreimageVersioned, Option<WrappedIr>) =
        tagged_deserialize(&mut &check[..]).unwrap();
    assert!(matches!(versioned, ProofPreimageVersioned::V2(_)));
    assert!(ir.is_none());
    Ok(())
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
    assert!(checked_total([1_usize, MAX_ARTIFACT_BYTES]).is_ok());
    assert_eq!(
        checked_total([MAX_ARTIFACT_BYTES; 5]),
        Err(LocalProverError::ResourcePreflightFailed)
    );
    assert_eq!(
        validate_location(""),
        Err(LocalProverError::InvalidConfiguration)
    );
    assert_eq!(
        validate_location(&"x".repeat(MAX_KEY_LOCATION_BYTES + 1)),
        Err(LocalProverError::InvalidConfiguration)
    );
    assert_eq!(
        request_preflight(&[]),
        Err(LocalProverError::ResourcePreflightFailed)
    );
    assert!(request_preflight(&[1]).is_ok());
    assert_eq!(
        validate_supplied_material(&material(vec![1, 2, 3])),
        Err(LocalProverError::UnsupportedCircuit)
    );
    assert_eq!(
        build_registry(&[], &[]).map(|_| ()),
        Err(LocalProverError::InvalidConfiguration)
    );
}

#[test]
fn prover_pool_uses_bounded_mobile_parallelism() {
    assert_eq!(
        prover_pool().unwrap().current_num_threads(),
        PROVER_THREAD_COUNT
    );
}

#[test]
fn providers_resolvers_and_registry_validation_cover_generic_inputs() {
    let fallback = material(vec![3]);
    let mut circuits = HashMap::new();
    circuits.insert("registered".to_owned(), fallback.clone());
    let registry = MemoryRegistry {
        params: HashMap::new(),
        circuits,
    };
    let missing = futures_executor::block_on(registry.get_params(9))
        .err()
        .unwrap();
    assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    assert!(
        futures_executor::block_on(
            registry.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("missing")))
        )
        .unwrap()
        .is_none()
    );
    let resolved = futures_executor::block_on(
        registry.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("registered"))),
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolved.ir_source, fallback.ir_source);

    let supplied = material(vec![4]);
    let resolver = RequestResolver {
        registry: &registry,
        supplied: Some(supplied.clone()),
    };
    let resolved = futures_executor::block_on(
        resolver.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("registered"))),
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolved.ir_source, supplied.ir_source);
    let fallback_resolver = RequestResolver {
        registry: &registry,
        supplied: None,
    };
    assert!(
        futures_executor::block_on(
            fallback_resolver.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("registered")))
        )
        .unwrap()
        .is_some()
    );

    let invalid_params = [0_u8; 4];
    let parameter = ParameterArtifact {
        k: 1,
        bytes: &invalid_params,
        sha256: &[0; 32],
    };
    assert_eq!(
        build_registry(&[parameter], &[]).map(|_| ()),
        Err(LocalProverError::IntegrityCheckFailed)
    );
    let invalid = b"invalid";
    let circuit_hash = Sha256::digest(invalid);
    let circuit = CircuitArtifact {
        key_location: "midnight/test",
        prover_key: invalid,
        prover_key_sha256: circuit_hash.as_ref(),
        verifier_key: invalid,
        verifier_key_sha256: circuit_hash.as_ref(),
        ir: invalid,
        ir_sha256: circuit_hash.as_ref(),
    };
    assert_eq!(
        decode_circuit(&circuit).map(|_| ()),
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
    let empty_inline = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(vec![])),
    ));
    assert_eq!(
        run_check(handle, &empty_inline),
        Err(LocalProverError::ResourcePreflightFailed)
    );
}

#[test]
fn valid_inline_and_registered_ir_cover_check_paths() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    let ir = minimal_ir_bytes(1);
    let inline = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(ir.clone())),
    ));
    let checked = run_check(handle, &inline).unwrap();
    let decoded: Vec<Option<u64>> = tagged_deserialize(&mut &checked[..]).unwrap();
    assert!(decoded.is_empty());

    registry_slot().lock().unwrap().value = Some(Arc::new(MemoryRegistry {
        params: HashMap::new(),
        circuits: HashMap::from([("registered/circuit".to_owned(), material(ir))]),
    }));
    let registered = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("registered/circuit")),
        None::<WrappedIr>,
    ));
    assert!(run_check(handle, &registered).is_ok());

    let mismatched = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(minimal_ir_bytes(2))),
    ));
    assert_eq!(
        run_check(handle, &mismatched),
        Err(LocalProverError::CheckFailed)
    );
}

#[test]
fn supplied_material_and_binding_cover_prove_failure_path() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    let unsupported = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("missing/circuit")),
        None::<ProvingKeyMaterial>,
        Some(Fr::from(7_u64)),
    ));
    assert_eq!(
        run_prove(handle, &unsupported),
        Err(LocalProverError::UnsupportedCircuit)
    );

    let invalid_material = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("supplied/circuit")),
        Some(material(vec![1, 2, 3])),
        None::<Fr>,
    ));
    assert_eq!(
        run_prove(handle, &invalid_material),
        Err(LocalProverError::UnsupportedCircuit)
    );

    let invalid_key = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("supplied/circuit")),
        Some(material(minimal_ir_bytes(1))),
        Some(Fr::from(11_u64)),
    ));
    assert_eq!(
        run_prove(handle, &invalid_key),
        Err(LocalProverError::ProofFailed)
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
    assert_eq!(close_registry(0), Err(LocalProverError::StaleRegistry));
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
    let output_params = read("bls_midnight_2p14");
    let prover = read("zswap/9/spend.prover");
    let verifier = read("zswap/9/spend.verifier");
    let ir = read("zswap/9/spend.bzkir");
    let output_prover = read("zswap/9/output.prover");
    let output_verifier = read("zswap/9/output.verifier");
    let output_ir = read("zswap/9/output.bzkir");
    let digest = |bytes: &[u8]| Sha256::digest(bytes).to_vec();
    let params_hash = digest(&params);
    let output_params_hash = digest(&output_params);
    let prover_hash = digest(&prover);
    let verifier_hash = digest(&verifier);
    let ir_hash = digest(&ir);
    let output_prover_hash = digest(&output_prover);
    let output_verifier_hash = digest(&output_verifier);
    let output_ir_hash = digest(&output_ir);
    let handle = configure_registry(
        &[
            ParameterArtifact {
                k: 15,
                bytes: &params,
                sha256: &params_hash,
            },
            ParameterArtifact {
                k: 14,
                bytes: &output_params,
                sha256: &output_params_hash,
            },
        ],
        &[
            CircuitArtifact {
                key_location: "midnight/zswap/spend",
                prover_key: &prover,
                prover_key_sha256: &prover_hash,
                verifier_key: &verifier,
                verifier_key_sha256: &verifier_hash,
                ir: &ir,
                ir_sha256: &ir_hash,
            },
            CircuitArtifact {
                key_location: "midnight/zswap/output",
                prover_key: &output_prover,
                prover_key_sha256: &output_prover_hash,
                verifier_key: &output_verifier,
                verifier_key_sha256: &output_verifier_hash,
                ir: &output_ir,
                ir_sha256: &output_ir_hash,
            },
        ],
    )
    .unwrap();
    let check = run_check(handle, &deterministic_zswap_spend_check_request().unwrap()).unwrap();
    let _: Vec<Option<u64>> = tagged_deserialize(&mut &check[..]).unwrap();
    let proof = run_prove(handle, &deterministic_zswap_spend_request().unwrap()).unwrap();
    let decoded: ProofVersioned = tagged_deserialize(&mut &proof[..]).unwrap();
    assert!(matches!(decoded, ProofVersioned::V2(_)));
    let output_proof = run_prove(handle, &deterministic_zswap_output_request().unwrap()).unwrap();
    let decoded: ProofVersioned = tagged_deserialize(&mut &output_proof[..]).unwrap();
    assert!(matches!(decoded, ProofVersioned::V2(_)));
    close_registry(handle).unwrap();
}
