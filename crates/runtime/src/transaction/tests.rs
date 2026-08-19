use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use midnight_ledger::structure::{INITIAL_PARAMETERS, Intent, ProofMarker, ProofPreimageVersioned};
use midnight_storage::storage::HashMap as LedgerHashMap;
use midnight_transient_crypto::proofs::{
    KeyLocation, ProofPreimage, ProvingKeyMaterial, ProvingProvider, WrappedIr,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::*;

fn serialized<T: Tagged + Serializable>(value: &T) -> Vec<u8> {
    let mut raw = Vec::new();
    tagged_serialize(value, &mut raw).unwrap();
    raw
}

fn empty_unproven(
    network: &str,
) -> Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> {
    Transaction::new(
        network.to_owned(),
        LedgerHashMap::new(),
        None,
        LedgerHashMap::new(),
    )
}

fn empty_proven(
    network: &str,
) -> Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB> {
    let responses = RemoteProofResponses::default();
    let key_material = RemoteProofKeyMaterials::default();
    let provider = PausingProver {
        responses: &responses,
        key_material: &key_material,
        captured: Arc::new(Mutex::new(BTreeMap::new())),
    };
    block_on(empty_unproven(network).prove(provider, &INITIAL_COST_MODEL)).unwrap()
}

fn synthetic_preimage(location: &'static str) -> ProofPreimage {
    ProofPreimage {
        inputs: vec![1_u64.into(), 2_u64.into()],
        private_transcript: vec![3_u64.into()],
        public_transcript_inputs: vec![4_u64.into()],
        public_transcript_outputs: vec![5_u64.into()],
        binding_input: 6_u64.into(),
        communications_commitment: None,
        key_location: KeyLocation(std::borrow::Cow::Borrowed(location)),
    }
}

fn proving_material(prover_key: u8, verifier_key: u8, ir_source: u8) -> ProvingKeyMaterial {
    ProvingKeyMaterial {
        prover_key: vec![prover_key],
        verifier_key: vec![verifier_key],
        ir_source: vec![ir_source],
    }
}

#[test]
fn empty_transactions_cover_complete_proof_and_finalization_paths() {
    let unproven = empty_unproven("preview");
    let raw = serialized(&unproven);
    assert!(
        validate_unproven_transaction(&raw, "preview")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        canonicalize_transaction("signature", "pre-proof", "pre-binding", &raw).unwrap(),
        raw
    );

    let complete =
        advance_unproven_transaction(&raw, "preview", &RemoteProofResponses::default()).unwrap();
    assert!(
        matches!(complete, BalanceProgress::Complete(_)),
        "an empty transaction must not request a proof"
    );
    let BalanceProgress::Complete(finalized) = complete else {
        return;
    };
    assert!(finalized.identifiers.is_empty());
    assert!(!finalized.transaction_hash.is_empty());
    assert_eq!(
        validate_finalized_transaction(&finalized.canonical, "preview")
            .unwrap()
            .transaction_hash,
        finalized.transaction_hash
    );
    assert_eq!(
        canonicalize_transaction("signature", "proof", "binding", &finalized.canonical).unwrap(),
        finalized.canonical
    );
    assert!(decode_balance_original(&finalized.canonical, true, "preview").is_ok());
}

#[test]
fn unsealed_empty_transactions_finalize_and_balance_without_dust() {
    let unsealed = empty_proven("preview");
    let raw = serialized(&unsealed);
    assert!(decode_balance_original(&raw, false, "preview").is_ok());
    let finalized = finalize_balance_original(&raw, false, "preview").unwrap();
    assert!(finalized.identifiers.is_empty());

    let original = empty_proven("preview");
    let balancing = empty_unproven("preview");
    let progress = advance_dust_balance(
        &serialized(&original),
        false,
        &serialized(&balancing),
        "preview",
        &RemoteProofResponses::default(),
    )
    .unwrap();
    assert!(matches!(progress, BalanceProgress::Complete(_)));
}

#[test]
fn transaction_decoders_reject_malformed_noncanonical_and_wrong_network_inputs() {
    let oversized = vec![0; MAX_TRANSACTION_BYTES + 1];
    for raw in [&[][..], &[0xff][..], oversized.as_slice()] {
        assert!(decode_ledger_parameters(raw).is_err());
        assert!(validate_unproven_transaction(raw, "preview").is_err());
        assert!(validate_finalized_transaction(raw, "preview").is_err());
    }

    let parameters = serialized(&INITIAL_PARAMETERS);
    assert_eq!(
        decode_ledger_parameters(&parameters).unwrap(),
        INITIAL_PARAMETERS
    );

    let raw = serialized(&empty_unproven("preview"));
    assert!(validate_unproven_transaction(&raw, "preprod").is_err());
    assert!(advance_unproven_transaction(&raw, "preprod", &Default::default()).is_err());
    assert!(advance_unproven_transaction(&raw, "", &Default::default()).is_err());
    assert!(decode_balance_original(&raw, false, "preview").is_err());

    for markers in [
        ("bad", "pre-proof", "pre-binding"),
        ("signature", "bad", "pre-binding"),
        ("signature", "pre-proof", "bad"),
    ] {
        assert!(canonicalize_transaction(markers.0, markers.1, markers.2, &raw).is_err());
    }
}

#[test]
fn canonicalization_accepts_all_supported_empty_transaction_shapes() {
    let variants = [
        (
            "signature",
            "pre-proof",
            "binding",
            serialized(&empty_unproven("preview").seal(StdRng::seed_from_u64(1))),
        ),
        (
            "signature-erased",
            "pre-proof",
            "pre-binding",
            serialized(&empty_unproven("preview").erase_signatures()),
        ),
        (
            "signature-erased",
            "pre-proof",
            "binding",
            serialized(
                &empty_unproven("preview")
                    .erase_signatures()
                    .seal(StdRng::seed_from_u64(2)),
            ),
        ),
        (
            "signature",
            "proof",
            "pre-binding",
            serialized(&empty_proven("preview")),
        ),
        (
            "signature-erased",
            "proof",
            "pre-binding",
            serialized(&empty_proven("preview").erase_signatures()),
        ),
        (
            "signature-erased",
            "proof",
            "binding",
            serialized(
                &empty_proven("preview")
                    .erase_signatures()
                    .seal(StdRng::seed_from_u64(3)),
            ),
        ),
        (
            "signature",
            "no-proof",
            "no-binding",
            serialized(&empty_unproven("preview").erase_proofs()),
        ),
        (
            "signature-erased",
            "no-proof",
            "no-binding",
            serialized(&empty_unproven("preview").erase_proofs().erase_signatures()),
        ),
    ];
    for (signature, proof, binding, raw) in variants {
        assert_eq!(
            canonicalize_transaction(signature, proof, binding, &raw).unwrap(),
            raw
        );
    }
}

#[test]
fn remote_response_registry_accepts_checks_and_rejects_invalid_proofs() {
    let check_values = vec![None, Some(3_u64)];
    let check_response = serialized(&check_values);
    let check = RemoteProofRequest {
        key: "check".to_owned(),
        kind: RemoteProofKind::Check,
        body: vec![1, 2, 3],
    };
    let prove = RemoteProofRequest {
        key: "prove".to_owned(),
        kind: RemoteProofKind::Prove,
        body: vec![4, 5, 6],
    };
    let mut responses = RemoteProofResponses::default();
    responses.accept(&check, check_response).unwrap();
    assert!(responses.accept(&check, Vec::new()).is_err());
    assert!(responses.accept(&prove, vec![0xff]).is_err());
}

#[test]
fn explicit_proof_material_maps_reject_empty_and_unknown_locations() {
    let mut invalid = RemoteProofKeyMaterials::default();
    assert!(
        invalid
            .insert(String::new(), proving_material(1, 2, 3))
            .is_err()
    );
    assert!(
        invalid
            .insert(
                "midnight/empty".to_owned(),
                ProvingKeyMaterial {
                    prover_key: Vec::new(),
                    verifier_key: vec![2],
                    ir_source: vec![3],
                },
            )
            .is_err()
    );

    let mut materials = RemoteProofKeyMaterials::default();
    materials
        .insert("midnight/unknown".to_owned(), proving_material(1, 2, 3))
        .unwrap();
    let raw = serialized(&empty_unproven("preview"));
    assert!(matches!(
        advance_unproven_transaction_with_materials(
            &raw,
            "preview",
            &RemoteProofResponses::default(),
            &materials,
        ),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
}

#[test]
fn pausing_prover_attaches_only_matching_material_to_check_and_prove_payloads() {
    let location = "midnight/test/circuit";
    let material = proving_material(11, 12, 13);
    let mut materials = RemoteProofKeyMaterials::default();
    materials
        .insert(location.to_owned(), material.clone())
        .unwrap();
    materials
        .insert(
            "midnight/test/other".to_owned(),
            proving_material(21, 22, 23),
        )
        .unwrap();
    let responses = RemoteProofResponses::default();
    let preimage = synthetic_preimage(location);

    let check_requests = Arc::new(Mutex::new(BTreeMap::new()));
    let check_provider = PausingProver {
        responses: &responses,
        key_material: &materials,
        captured: Arc::clone(&check_requests),
    };
    assert!(block_on(check_provider.check(&preimage)).is_err());
    let check_body = check_requests
        .lock()
        .unwrap()
        .values()
        .find(|request| request.kind == RemoteProofKind::Check)
        .unwrap()
        .body
        .clone();
    let (check_preimage, ir): (ProofPreimageVersioned, Option<WrappedIr>) =
        tagged_deserialize(&mut &check_body[..]).unwrap();
    assert!(matches!(check_preimage, ProofPreimageVersioned::V2(_)));
    assert_eq!(ir.unwrap().0, material.ir_source);

    let prove_requests = Arc::new(Mutex::new(BTreeMap::new()));
    let prove_provider = PausingProver {
        responses: &responses,
        key_material: &materials,
        captured: Arc::clone(&prove_requests),
    };
    assert!(block_on(prove_provider.prove(&preimage, None)).is_err());
    let prove_body = prove_requests
        .lock()
        .unwrap()
        .values()
        .find(|request| request.kind == RemoteProofKind::Prove)
        .unwrap()
        .body
        .clone();
    let (prove_preimage, attached, binding): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<midnight_transient_crypto::curve::Fr>,
    ) = tagged_deserialize(&mut &prove_body[..]).unwrap();
    assert!(matches!(prove_preimage, ProofPreimageVersioned::V2(_)));
    assert!(binding.is_none());
    let attached = attached.unwrap();
    assert_eq!(attached.prover_key, material.prover_key);
    assert_eq!(attached.verifier_key, material.verifier_key);
    assert_eq!(attached.ir_source, material.ir_source);
}

/// The executor itself is covered in `crate::executor::tests`; this asserts the
/// prove tree it drives reaches a terminal state.
#[test]
fn drives_an_empty_transaction_to_a_terminal_state() {
    let mut rng = StdRng::seed_from_u64(7);
    let intent = Intent::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::empty(
        &mut rng,
        midnight_base_crypto::time::Timestamp::from_secs(10),
    );
    let transaction = Transaction::from_intents("preview", [(1_u16, intent)].into_iter().collect());
    assert!(matches!(
        advance_unproven_transaction(
            &serialized(&transaction),
            "preview",
            &RemoteProofResponses::default()
        ),
        Ok(BalanceProgress::Complete(_)) | Ok(BalanceProgress::Network(_))
    ));
}
