use super::hardening::*;
use super::*;

fn encode_initial_parameters() -> String {
    let mut parameters = Vec::new();
    midnight_serialize::tagged_serialize(
        &midnight_ledger::structure::INITIAL_PARAMETERS,
        &mut parameters,
    )
    .unwrap();
    encode_base64(&parameters)
}

fn empty_unproven_transaction() -> Vec<u8> {
    let transaction = midnight_ledger::structure::Transaction::<
        midnight_base_crypto::schnorr::Signature,
        midnight_ledger::structure::ProofPreimageMarker,
        midnight_transient_crypto::commitment::PedersenRandomness,
        midnight_storage::db::InMemoryDB,
    >::new(
        "preview".to_owned(),
        midnight_storage::storage::HashMap::new(),
        None,
        midnight_storage::storage::HashMap::new(),
    );
    let mut raw = Vec::new();
    midnight_serialize::tagged_serialize(&transaction, &mut raw).unwrap();
    raw
}

fn empty_finalized_transaction() -> transaction::FinalizedTransaction {
    let raw = empty_unproven_transaction();
    let progress =
        transaction::advance_unproven_transaction(&raw, "preview", &Default::default()).unwrap();
    let finalized = match progress {
        transaction::BalanceProgress::Complete(finalized) => Some(finalized),
        transaction::BalanceProgress::Network(_) => None,
    };
    assert!(
        finalized.is_some(),
        "empty transaction unexpectedly requested a proof"
    );
    finalized.unwrap()
}

fn accepted_check_result(effect_id: &str) -> String {
    let mut body = Vec::new();
    midnight_serialize::tagged_serialize(&vec![None::<u64>], &mut body).unwrap();
    serde_json::json!({
        "effectId": effect_id,
        "outcome": "accepted",
        "bodyBase64": encode_base64(&body)
    })
    .to_string()
}

fn proof_operation_kinds(
    wallet_state: &NativeWalletState,
    unproven: &[u8],
) -> Vec<PendingOperationKind> {
    let request = || transaction::RemoteProofRequest {
        key: "synthetic-check".to_owned(),
        kind: transaction::RemoteProofKind::Check,
        body: vec![1],
    };
    vec![
        PendingOperationKind::FinalizeTransactionProof {
            raw: unproven.to_vec(),
            key_material: transaction::RemoteProofKeyMaterials::default(),
            proposed_state: Some(wallet_state.clone()),
            expected_identifiers: Vec::new(),
            responses: transaction::RemoteProofResponses::default(),
            pending_requests: vec![request()],
        },
        PendingOperationKind::DappIntentProof {
            raw: unproven.to_vec(),
            responses: transaction::RemoteProofResponses::default(),
            pending_requests: vec![request()],
        },
        PendingOperationKind::GenerateDustProof {
            raw: unproven.to_vec(),
            proposed_state: wallet_state.clone(),
            expected_identifiers: Vec::new(),
            responses: transaction::RemoteProofResponses::default(),
            pending_requests: vec![request()],
        },
        PendingOperationKind::Balance {
            original_raw: vec![0xff],
            original_sealed: false,
            balancing_raw: unproven.to_vec(),
            responses: transaction::RemoteProofResponses::default(),
            pending_requests: vec![request()],
        },
    ]
}

fn shielded_receiver() -> String {
    use bech32::{Bech32m, Hrp};
    use midnight_zswap::keys::{SecretKeys, Seed};

    let keys = SecretKeys::from(Seed::from([2; 32]));
    let mut bytes = hex::decode(serializable_hex(&keys.coin_public_key()).unwrap()).unwrap();
    bytes.extend(hex::decode(serializable_hex(&keys.enc_public_key()).unwrap()).unwrap());
    bech32::encode::<Bech32m>(Hrp::parse("mn_shield-addr_preview").unwrap(), &bytes).unwrap()
}

fn unshielded_receiver() -> String {
    use bech32::{Bech32m, Hrp};
    use midnight_coin_structure::coin::UserAddress;

    let key = SigningKey::from_bytes(&[1; 32]).unwrap();
    let address = UserAddress::from(key.verifying_key());
    let bytes = hex::decode(serializable_hex(&address).unwrap()).unwrap();
    bech32::encode::<Bech32m>(Hrp::parse("mn_addr_preview").unwrap(), &bytes).unwrap()
}

fn mark_all_streams_caught_up(handle: &RuntimeSessionHandle) {
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    let mut state = lock_session(&session).unwrap();
    state.caught_up_streams = ["shielded", "unshielded", "dust"]
        .into_iter()
        .map(str::to_owned)
        .collect();
}

#[test]
fn transaction_command_families_cover_real_and_invalid_dispatch_paths() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("coverage-transaction-commands", None);

    for command in [
        serde_json::json!({"kind": "dappTransfer", "outputs": []}),
        serde_json::json!({"kind": "dappIntent", "inputs": [], "outputs": []}),
        serde_json::json!({
            "kind": "transfer",
            "to": "receiver",
            "amount": "1",
            "tokenType": "token",
            "walletType": "shielded"
        }),
        serde_json::json!({
            "kind": "generateDust",
            "ledgerParametersBase64": "",
            "feeBlocksMargin": 0,
            "additionalFeeOverhead": "0"
        }),
        serde_json::json!({
            "kind": "balanceUnsealed",
            "rawBase64": "",
            "ledgerParametersBase64": "",
            "feeBlocksMargin": 0,
            "additionalFeeOverhead": "0",
            "feeMode": "localDust",
            "approvedManifest": approved_manifest()
        }),
        serde_json::json!({
            "kind": "previewBalance",
            "rawBase64": "",
            "sealed": false,
            "ledgerParametersBase64": "",
            "feeBlocksMargin": 0,
            "additionalFeeOverhead": "0",
            "feeMode": "localDust"
        }),
    ] {
        assert_runtime_error(
            begin_command(handle.id, handle.generation, command.to_string()),
            "UNAVAILABLE",
        );
    }

    mark_all_streams_caught_up(&handle);
    for command in [
        serde_json::json!({
            "kind": "transfer",
            "to": "receiver",
            "amount": "not-a-number",
            "tokenType": "token",
            "walletType": "shielded"
        }),
        serde_json::json!({
            "kind": "transfer",
            "to": "receiver",
            "amount": "1",
            "tokenType": "token",
            "walletType": "dust"
        }),
        serde_json::json!({
            "kind": "dappTransfer",
            "outputs": [{
                "walletType": "shielded",
                "tokenType": "token",
                "amount": "-1",
                "receiverAddress": "receiver"
            }]
        }),
        serde_json::json!({
            "kind": "dappIntent",
            "inputs": [{"walletType": "shielded", "tokenType": "token", "amount": "-1"}],
            "outputs": []
        }),
        serde_json::json!({
            "kind": "generateDust",
            "ledgerParametersBase64": "not-base64",
            "feeBlocksMargin": 0,
            "additionalFeeOverhead": "0"
        }),
        serde_json::json!({
            "kind": "balanceUnsealed",
            "rawBase64": "",
            "ledgerParametersBase64": "not-base64",
            "feeBlocksMargin": 0,
            "additionalFeeOverhead": "0",
            "feeMode": "localDust",
            "approvedManifest": approved_manifest()
        }),
        serde_json::json!({
            "kind": "balanceSealed",
            "rawBase64": "",
            "ledgerParametersBase64": "not-base64",
            "feeBlocksMargin": 0,
            "additionalFeeOverhead": "0",
            "feeMode": "localDust",
            "approvedManifest": approved_manifest()
        }),
        serde_json::json!({"kind": "submitFinalized", "rawBase64": "not-base64"}),
    ] {
        assert_runtime_error(
            begin_command(handle.id, handle.generation, command.to_string()),
            "INVALID_ARGUMENT",
        );
    }

    for command in [
        serde_json::json!({"kind": "dappTransfer", "outputs": []}),
        serde_json::json!({"kind": "dappIntent", "inputs": [], "outputs": []}),
    ] {
        assert_runtime_error(
            begin_command(handle.id, handle.generation, command.to_string()),
            "INVALID_ARGUMENT",
        );
    }

    assert_runtime_error(
        begin_command(
            handle.id,
            handle.generation,
            serde_json::json!({
                "kind": "generateDust",
                "ledgerParametersBase64": encode_initial_parameters(),
                "feeBlocksMargin": 0,
                "additionalFeeOverhead": "0"
            })
            .to_string(),
        ),
        "INVALID_ARGUMENT",
    );
}

/// A manifest that can never authorize a real plan: it is only here so the
/// command decodes and the validation under test is the one that runs.
fn approved_manifest() -> serde_json::Value {
    serde_json::json!({
        "transactionDigest": "00".repeat(32),
        "variant": "unsealed",
        "contributions": [],
        "change": [],
        "dust": "0",
        "walletStateDigest": "00".repeat(32)
    })
}

#[test]
fn transaction_handlers_validate_canonical_inputs_before_wallet_mutation() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("coverage-validated-commands", None);
    mark_all_streams_caught_up(&handle);
    let parameters = encode_initial_parameters();
    let finalized = empty_finalized_transaction();
    let snapshot = get_wallet_snapshot(handle.id, handle.generation).unwrap();

    // A margin above the ceiling and a non-canonical overhead are argument
    // errors. The fixture is a sealed transaction, so asking for the unsealed
    // variant is also an argument error. Only the sealed request with
    // well-formed arguments reaches coin selection, where an empty wallet
    // reports the DUST shortfall specifically rather than generically.
    for (kind, fee_blocks_margin, overhead, expected) in [
        ("balanceUnsealed", 65, "0", "INVALID_ARGUMENT"),
        ("balanceSealed", 65, "0", "INVALID_ARGUMENT"),
        ("balanceUnsealed", 0, "01", "INVALID_ARGUMENT"),
        ("balanceSealed", 0, "01", "INVALID_ARGUMENT"),
        ("balanceUnsealed", 0, "0", "INVALID_ARGUMENT"),
        ("balanceSealed", 0, "0", "INSUFFICIENT_DUST"),
    ] {
        assert_runtime_error(
            begin_command(
                handle.id,
                handle.generation,
                serde_json::json!({
                    "kind": kind,
                    "rawBase64": encode_base64(&finalized.canonical),
                    "ledgerParametersBase64": parameters,
                    "feeBlocksMargin": fee_blocks_margin,
                    "additionalFeeOverhead": overhead,
                    "feeMode": "localDust",
                    "approvedManifest": approved_manifest()
                })
                .to_string(),
            ),
            expected,
        );
    }

    for command in [
        serde_json::json!({
            "kind": "transfer",
            "to": shielded_receiver(),
            "amount": "1",
            "tokenType": "08".repeat(32),
            "walletType": "shielded"
        }),
        serde_json::json!({
            "kind": "transfer",
            "to": unshielded_receiver(),
            "amount": "1",
            "tokenType": "00".repeat(32),
            "walletType": "unshielded"
        }),
        serde_json::json!({
            "kind": "dappTransfer",
            "outputs": [{
                "walletType": "shielded",
                "tokenType": "08".repeat(32),
                "amount": "1",
                "receiverAddress": shielded_receiver()
            }]
        }),
        serde_json::json!({
            "kind": "dappIntent",
            "inputs": [{
                "walletType": "shielded",
                "tokenType": "08".repeat(32),
                "amount": "1"
            }],
            "outputs": []
        }),
        serde_json::json!({
            "kind": "generateDust",
            "ledgerParametersBase64": parameters,
            "feeBlocksMargin": 65,
            "additionalFeeOverhead": "0"
        }),
        serde_json::json!({
            "kind": "generateDust",
            "ledgerParametersBase64": parameters,
            "feeBlocksMargin": 0,
            "additionalFeeOverhead": "01"
        }),
    ] {
        assert_runtime_error(
            begin_command(handle.id, handle.generation, command.to_string()),
            "INVALID_ARGUMENT",
        );
    }
    assert_eq!(
        get_wallet_snapshot(handle.id, handle.generation).unwrap(),
        snapshot
    );
}

#[test]
fn submit_finalized_command_tracks_and_completes_a_real_empty_transaction() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("coverage-submit-command", None);
    let finalized = empty_finalized_transaction();
    let step = run_command(
        &handle,
        serde_json::json!({
            "kind": "submitFinalized",
            "rawBase64": encode_base64(&finalized.canonical)
        }),
    )
    .unwrap();
    assert_eq!(step["kind"], "network");
    assert_eq!(step["effect"], "submit");
    assert_runtime_error(
        begin_command(
            handle.id,
            handle.generation,
            serde_json::json!({
                "kind": "submitFinalized",
                "rawBase64": encode_base64(&finalized.canonical)
            })
            .to_string(),
        ),
        "SUBMISSION_STATUS_UNKNOWN",
    );
    let resumed = resume_operation(
        step["operation"]["id"].as_u64().unwrap(),
        step["operation"]["generation"].as_u64().unwrap(),
        Some(
            serde_json::json!({
                "effectId": step["effectId"],
                "outcome": "accepted"
            })
            .to_string(),
        ),
    )
    .unwrap();
    let resumed: serde_json::Value = serde_json::from_str(&resumed).unwrap();
    assert_eq!(resumed["kind"], "complete");
    assert_eq!(resumed["result"]["status"], "accepted");
}

#[test]
fn accepted_proof_responses_drive_each_resumable_transaction_family() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("coverage-accepted-resumes", None);
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    let wallet_state = lock_session(&session).unwrap().wallet_state.clone();
    let unproven = empty_unproven_transaction();

    for kind in proof_operation_kinds(&wallet_state, &unproven) {
        let (operation, effect_id) =
            register_operation(handle.id, handle.generation, &session, kind).unwrap();
        let result = resume_operation(
            operation.id,
            operation.generation,
            Some(accepted_check_result(&effect_id)),
        );
        if lock_session(&session).unwrap().active_operation.is_some() {
            cancel_operation(operation.id, operation.generation).unwrap();
        }
        match result {
            Ok(step) => {
                let step: serde_json::Value = serde_json::from_str(&step).unwrap();
                assert!(matches!(
                    step["kind"].as_str(),
                    Some("network" | "complete")
                ));
            }
            Err(error) => assert!(matches!(error, MidnightRuntimeError::ProofFailed)),
        }
    }

    for kind in proof_operation_kinds(&wallet_state, &unproven) {
        let (operation, effect_id) =
            register_operation(handle.id, handle.generation, &session, kind).unwrap();
        let error = resume_operation(
            operation.id,
            operation.generation,
            Some(
                serde_json::json!({
                    "effectId": effect_id,
                    "outcome": "unexpected"
                })
                .to_string(),
            ),
        )
        .unwrap_err();
        assert!(matches!(error, MidnightRuntimeError::InvalidArgument));
        assert_eq!(
            lock_session(&session).unwrap().active_operation,
            Some(operation.id)
        );
        cancel_operation(operation.id, operation.generation).unwrap();
    }

    for kind in proof_operation_kinds(&wallet_state, &unproven) {
        let (operation, effect_id) =
            register_operation(handle.id, handle.generation, &session, kind).unwrap();
        let error = resume_operation(
            operation.id,
            operation.generation,
            Some(
                serde_json::json!({
                    "effectId": effect_id,
                    "outcome": "accepted",
                    "bodyBase64": "AA=="
                })
                .to_string(),
            ),
        )
        .unwrap_err();
        assert!(matches!(error, MidnightRuntimeError::ProofFailed));
        assert!(lock_session(&session).unwrap().active_operation.is_none());
    }
}
