use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use midnight_ledger::structure::{INITIAL_PARAMETERS, Intent, ProofMarker};
use midnight_storage::storage::HashMap as LedgerHashMap;
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
    let provider = PausingProver {
        responses: Arc::new(RemoteProofResponses::default()),
        captured: Arc::new(Mutex::new(BTreeMap::new())),
    };
    block_on(empty_unproven(network).prove(provider, &INITIAL_COST_MODEL)).unwrap()
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
fn block_on_handles_pending_futures() {
    struct WakeOnce(bool);
    impl Future for WakeOnce {
        type Output = u8;

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<Self::Output> {
            if self.0 {
                Poll::Ready(9)
            } else {
                self.0 = true;
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    assert_eq!(block_on(WakeOnce(false)), 9);
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
