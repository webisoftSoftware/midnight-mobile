use super::hardening::*;
use super::*;
use bech32::{Bech32m, Hrp};
use midnight_coin_structure::coin::UserAddress;
use midnight_ledger::structure::INITIAL_PARAMETERS;
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

fn mark_all_streams_caught_up(handle: &RuntimeSessionHandle) {
    for stream in ["shielded-tip", "unshielded-tip", "dust-tip"] {
        apply_sync_batch(
            handle.id,
            handle.generation,
            stream.to_owned(),
            0,
            0,
            Vec::new(),
        )
        .unwrap();
    }
}

fn unshielded_target() -> String {
    let address = UserAddress::from(SigningKey::from_bytes(&[9; 32]).unwrap().verifying_key());
    bech32::encode::<Bech32m>(
        Hrp::parse("mn_addr_preview").unwrap(),
        &hex::decode(serializable_hex(&address).unwrap()).unwrap(),
    )
    .unwrap()
}

fn unshielded_created(output_no: u32, value: u128) -> Vec<u8> {
    serde_json::json!({
        "type": "UnshieldedTransaction",
        "transaction": {
            "id": output_no,
            "type": "RegularTransaction",
            "transactionResult": { "status": "SUCCESS" }
        },
        "createdUtxos": [{
            "value": value.to_string(),
            "owner": "self",
            "tokenType": "00".repeat(32),
            "intentHash": format!("{output_no:02x}").repeat(32),
            "outputIndex": output_no,
            "ctime": 1,
            "registeredForDustGeneration": false
        }],
        "spentUtxos": []
    })
    .to_string()
    .into_bytes()
}

fn add_shielded_coin(handle: &RuntimeSessionHandle, value: u128) -> String {
    let runtime = lock_registry().unwrap();
    let session = runtime.sessions.get(&handle.id).unwrap();
    let mut state = lock_session(session).unwrap();
    let zswap_seed = state.secrets.zswap_seed.clone();
    state
        .wallet_state
        .insert_test_shielded_coin(&zswap_seed, value)
        .unwrap();
    let mut address = hex::decode(&state.address_material.shielded_coin_public_key_hex).unwrap();
    address
        .extend(hex::decode(&state.address_material.shielded_encryption_public_key_hex).unwrap());
    bech32::encode::<Bech32m>(Hrp::parse("mn_shield-addr_preview").unwrap(), &address).unwrap()
}

#[test]
fn shielded_transfer_starts_with_an_explicit_proof_effect() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("transfer-finalization", None);
    mark_all_streams_caught_up(&handle);
    let target = add_shielded_coin(&handle, 10);
    let first: Value = serde_json::from_str(
        &begin_command(
            handle.id,
            handle.generation,
            serde_json::json!({
                "kind": "transfer",
                "walletType": "shielded",
                "to": target,
                "amount": "4",
                "tokenType": "00".repeat(32),
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(first["effect"].as_str(), Some("check" | "prove")));
    assert_ne!(first["effect"], "proveAndBalance");
    cancel_operation(
        first["operation"]["id"].as_u64().unwrap(),
        handle.generation,
    )
    .unwrap();
}

#[test]
fn dapp_transfer_reserves_the_session_and_uses_split_proof_effects() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("transfer-finalization", None);
    mark_all_streams_caught_up(&handle);
    let target = add_shielded_coin(&handle, 10);
    let mut parameters = Vec::new();
    tagged_serialize(&INITIAL_PARAMETERS, &mut parameters).unwrap();
    let command = serde_json::json!({
        "kind": "dappTransfer",
        "outputs": [{
            "walletType": "shielded",
            "tokenType": "00".repeat(32),
            "amount": "4",
            "receiverAddress": target,
        }],
        "ledgerParametersBase64": encode_base64(&parameters),
        "feeBlocksMargin": 1,
        "additionalFeeOverhead": "0",
    })
    .to_string();

    let first: Value = serde_json::from_str(
        &begin_command(handle.id, handle.generation, command.clone()).unwrap(),
    )
    .unwrap();
    assert!(matches!(first["effect"].as_str(), Some("check" | "prove")));
    assert_ne!(first["effect"], "proveAndBalance");
    assert_eq!(first["endpointRole"], "proof");
    {
        let runtime = lock_registry().unwrap();
        let PendingOperationKind::FinalizeTransactionProof { local_balance, .. } = &runtime
            .operations
            .get(&first["operation"]["id"].as_u64().unwrap())
            .unwrap()
            .kind
        else {
            unreachable!("expected transfer proof operation")
        };
        assert!(local_balance.is_some());
    }

    assert_eq!(
        begin_command(handle.id, handle.generation, command)
            .unwrap_err()
            .to_string(),
        "UNAVAILABLE"
    );
    assert_eq!(
        resume_operation(
            first["operation"]["id"].as_u64().unwrap(),
            handle.generation,
            Some(
                serde_json::json!({
                    "effectId": "wrong-effect-id",
                    "outcome": "accepted",
                    "bodyBase64": encode_base64(&[1]),
                })
                .to_string()
            ),
        )
        .unwrap_err()
        .to_string(),
        "INVALID_ARGUMENT"
    );
    cancel_operation(
        first["operation"]["id"].as_u64().unwrap(),
        handle.generation,
    )
    .unwrap();
}

fn add_dust_utxo(handle: &RuntimeSessionHandle, index: u64, initial_value: u128) {
    let now_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    let mut state = lock_session(&session).unwrap();
    let dust_seed = state.secrets.dust_seed.clone();
    state
        .wallet_state
        .insert_test_dust_utxo(&dust_seed, index, initial_value, now_seconds)
        .unwrap();
}

fn begin_unsponsored_unshielded_transfer(
    handle: &RuntimeSessionHandle,
    with_dust: bool,
) -> Result<Value, MidnightRuntimeError> {
    apply_sync_batch(
        handle.id,
        handle.generation,
        "unshielded".into(),
        0,
        1,
        vec![unshielded_created(1, 8)],
    )?;
    for (stream, offset) in [("shielded-tip", 0), ("dust-tip", 0), ("unshielded-tip", 1)] {
        apply_sync_batch(
            handle.id,
            handle.generation,
            stream.into(),
            offset,
            offset,
            Vec::new(),
        )?;
    }
    if with_dust {
        add_dust_utxo(handle, 0, u128::MAX / 8);
    }
    let mut parameters = Vec::new();
    tagged_serialize(&INITIAL_PARAMETERS, &mut parameters).unwrap();
    serde_json::from_str(&begin_command(
        handle.id,
        handle.generation,
        serde_json::json!({
            "kind": "transfer",
            "walletType": "unshielded",
            "to": unshielded_target(),
            "amount": "4",
            "tokenType": "00".repeat(32),
            "ledgerParametersBase64": encode_base64(&parameters),
            "feeBlocksMargin": 1,
            "additionalFeeOverhead": "0",
        })
        .to_string(),
    )?)
    .map_err(|_| MidnightRuntimeError::NativeInternal)
}

fn mocked_dust_proof(operation_id: u64) -> Vec<u8> {
    use crate::compat::Signature;
    use midnight_ledger::structure::{ProofPreimageMarker, Transaction};
    use midnight_storage::db::InMemoryDB;
    use midnight_transient_crypto::commitment::PedersenRandomness;

    let balancing_raw = {
        let runtime = lock_registry().unwrap();
        let PendingOperationKind::Balance { balancing_raw, .. } =
            &runtime.operations.get(&operation_id).unwrap().kind
        else {
            unreachable!("expected local DUST balance operation")
        };
        balancing_raw.clone()
    };
    let balancing: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&balancing_raw[..]).unwrap();
    let mocked = balancing.mock_prove().unwrap();
    let Transaction::Standard(standard) = mocked else {
        unreachable!("DUST balance must be a standard transaction")
    };
    let proof = &standard
        .intents
        .values()
        .next()
        .unwrap()
        .dust_actions
        .as_ref()
        .unwrap()
        .spends
        .iter()
        .next()
        .unwrap()
        .proof;
    let mut encoded = Vec::new();
    tagged_serialize(proof, &mut encoded).unwrap();
    encoded
}

#[test]
fn unsponsored_transfer_pays_its_own_dust_instead_of_calling_the_balance_service() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("transfer-finalization", None);
    let mut step = begin_unsponsored_unshielded_transfer(&handle, true).unwrap();

    // Local DUST balancing proves the fee-paying transaction.
    // The balance service does not sponsor this transfer.
    assert_eq!(step["endpointRole"], "proof");
    assert!(matches!(step["effect"].as_str(), Some("check" | "prove")));
    assert_ne!(step["effect"], "balance");
    assert_ne!(step["effect"], "proveAndBalance");
    let snapshot: Value =
        serde_json::from_str(&get_wallet_snapshot(handle.id, handle.generation).unwrap()).unwrap();
    assert_eq!(
        snapshot["balances"]["dustCoins"].as_array().unwrap().len(),
        1
    );
    assert_eq!(snapshot["balances"]["availableUtxos"], 1);

    let operation_id = step["operation"]["id"].as_u64().unwrap();
    let proof = mocked_dust_proof(operation_id);
    for _ in 0..8 {
        if step["kind"] == "complete" {
            break;
        }
        let response = match step["effect"].as_str() {
            Some("check") => {
                let mut response = Vec::new();
                tagged_serialize(&vec![None::<u64>; 57], &mut response).unwrap();
                response
            }
            Some("prove") => proof.clone(),
            effect => unreachable!("unexpected local balance effect: {effect:?}"),
        };
        step = serde_json::from_str(
            &resume_operation(
                operation_id,
                handle.generation,
                Some(
                    serde_json::json!({
                        "effectId": step["effectId"],
                        "outcome": "accepted",
                        "bodyBase64": encode_base64(&response),
                    })
                    .to_string(),
                ),
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(step["kind"], "complete");
    let committed: Value =
        serde_json::from_str(&get_wallet_snapshot(handle.id, handle.generation).unwrap()).unwrap();
    assert_eq!(
        committed["balances"]["dustCoins"].as_array().unwrap().len(),
        0
    );
    assert_eq!(committed["balances"]["availableUtxos"], 0);
    assert!(
        lock_session(&session_for_handle(handle.id, handle.generation).unwrap())
            .unwrap()
            .active_operation
            .is_none()
    );
}

#[test]
fn failed_local_dust_proofs_do_not_commit_and_release_the_reservation() {
    let _runtime = isolated_runtime();
    for (outcome, body_base64) in [
        ("accepted", Some(encode_base64(&[0xff]))),
        ("rejected", None),
    ] {
        let handle = open_test_session("transfer-finalization", None);
        let step = begin_unsponsored_unshielded_transfer(&handle, true).unwrap();
        let result = resume_operation(
            step["operation"]["id"].as_u64().unwrap(),
            handle.generation,
            Some(
                serde_json::json!({
                    "effectId": step["effectId"],
                    "outcome": outcome,
                    "bodyBase64": body_base64,
                })
                .to_string(),
            ),
        );
        assert_eq!(result.unwrap_err().to_string(), "PROOF_FAILED");
        let snapshot: Value =
            serde_json::from_str(&get_wallet_snapshot(handle.id, handle.generation).unwrap())
                .unwrap();
        assert_eq!(
            snapshot["balances"]["dustCoins"].as_array().unwrap().len(),
            1
        );
        assert_eq!(snapshot["balances"]["availableUtxos"], 1);
        assert!(
            lock_session(&session_for_handle(handle.id, handle.generation).unwrap())
                .unwrap()
                .active_operation
                .is_none()
        );
    }
}

#[test]
fn unsponsored_transfer_without_enough_dust_fails_typed_and_releases_reservation() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("transfer-finalization", None);
    assert_eq!(
        begin_unsponsored_unshielded_transfer(&handle, false)
            .unwrap_err()
            .to_string(),
        "INSUFFICIENT_DUST"
    );
    let snapshot: Value =
        serde_json::from_str(&get_wallet_snapshot(handle.id, handle.generation).unwrap()).unwrap();
    assert_eq!(snapshot["balances"]["availableUtxos"], 1);
    assert!(
        lock_session(&session_for_handle(handle.id, handle.generation).unwrap())
            .unwrap()
            .active_operation
            .is_none()
    );
}
