use bech32::{Bech32m, Hrp};
use midnight_ledger::structure::INITIAL_PARAMETERS;
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::*;

const NIGHT: &str = "0000000000000000000000000000000000000000000000000000000000000000";

mod sync;

fn material() -> WalletAddressMaterial {
    super::super::derive_wallet_address_material_inner(&[1; 32], &[2; 32], &[3; 32]).unwrap()
}

fn empty_state() -> NativeWalletState {
    let address_material = material();
    NativeWalletState::restore(
        &LegacyWalletState::default(),
        RestoreContext {
            network_id: "preview",
            unshielded_address: "synthetic-address",
            address_material: &address_material,
            night_external_key: &[1; 32],
        },
    )
    .unwrap()
}

fn unshielded_address(network: &str) -> String {
    let signing_key = SigningKey::from_bytes(&[1; 32]).unwrap();
    let address = UserAddress::from(signing_key.verifying_key());
    let bytes = hex::decode(serializable_hex(&address).unwrap()).unwrap();
    let hrp = Hrp::parse(&format!("mn_addr_{network}")).unwrap();
    bech32::encode::<Bech32m>(hrp, &bytes).unwrap()
}

fn shielded_address(keys: &ZswapSecretKeys, network: &str) -> String {
    let mut bytes = hex::decode(serializable_hex(&keys.coin_public_key()).unwrap()).unwrap();
    bytes.extend(hex::decode(serializable_hex(&keys.enc_public_key()).unwrap()).unwrap());
    let hrp = Hrp::parse(&format!("mn_shield-addr_{network}")).unwrap();
    bech32::encode::<Bech32m>(hrp, &bytes).unwrap()
}

fn add_unshielded(state: &mut NativeWalletState, value: &str, token_type: &str, output_index: u32) {
    let update = serde_json::json!({
        "type": "UnshieldedTransaction",
        "transaction": {
            "id": u64::from(output_index) + 1,
            "type": "SystemTransaction",
            "block": { "timestamp": 1_000 }
        },
        "createdUtxos": [{
            "value": value,
            "owner": "synthetic-owner",
            "tokenType": token_type,
            "intentHash": format!("{:064x}", output_index + 1),
            "outputIndex": output_index,
            "ctime": 1,
            "registeredForDustGeneration": false
        }],
        "spentUtxos": []
    });
    state
        .apply_unshielded(&serde_json::to_vec(&update).unwrap())
        .unwrap();
}

fn funded_state() -> (NativeWalletState, ZswapSecretKeys, String) {
    let mut state = empty_state();
    add_unshielded(&mut state, "1000000000000", NIGHT, 0);
    add_unshielded(&mut state, "500", &"09".repeat(32), 1);

    let keys = ZswapSecretKeys::from(ZswapSeed::from([2; 32]));
    let mut rng = StdRng::seed_from_u64(11);
    let coin = ShieldedCoinInfo::new(&mut rng, 700, ShieldedTokenType(HashOutput([8; 32])));
    let output = ZswapOutput::<ProofPreimage, InMemoryDB>::new(
        &mut rng,
        &coin,
        Some(0),
        &keys.coin_public_key(),
        Some(keys.enc_public_key()),
    )
    .unwrap();
    let offer = ZswapOffer::new(Vec::new(), vec![output], Vec::new()).unwrap();
    state.shielded = state
        .shielded
        .watch_for(&keys.coin_public_key(), &coin)
        .apply(&keys, &offer)
        .unwrap();
    state.refresh_coin_hashes(&keys).unwrap();
    let address = shielded_address(&keys, "preview");
    (state, keys, address)
}

#[test]
fn parsing_helpers_accept_canonical_values_and_reject_malformed_input() {
    for value in ["0", "1", "10", "999999"] {
        assert!(canonical_decimal(value));
    }
    for value in ["", "00", "01", "-1", "+1", " 1"] {
        assert!(!canonical_decimal(value));
    }
    assert!(nonempty_bounded("value", 5));
    assert!(!nonempty_bounded("", 5));
    assert!(!nonempty_bounded("value", 4));

    let state = midnight_zswap::local::State::<InMemoryDB>::new();
    let encoded = encode_hex_state(&state).unwrap();
    let decoded: midnight_zswap::local::State<InMemoryDB> = decode_hex_state(&encoded).unwrap();
    assert_eq!(decoded.coins.iter().count(), 0);
    assert!(decode_hex_state::<midnight_zswap::local::State<InMemoryDB>>("").is_err());
    assert!(decode_hex_state::<midnight_zswap::local::State<InMemoryDB>>("xyz").is_err());
    assert!(parse_snapshot::<serde_json::Value>("").is_err());
    assert!(parse_snapshot::<serde_json::Value>("{").is_err());
    assert_eq!(
        to_snapshot_json(&serde_json::json!({"ok": true})).unwrap(),
        r#"{"ok":true}"#
    );

    assert!(decode_hash(NIGHT).is_ok());
    assert!(decode_hash("00").is_err());
    assert!(decode_unshielded_address(&unshielded_address("preview"), "preview").is_ok());
    assert!(decode_unshielded_address(&unshielded_address("preview"), "preprod").is_err());
    assert!(decode_unshielded_address("not-an-address", "preview").is_err());
}

#[test]
fn shielded_mint_hooks_derive_keys_and_watch_only_canonical_current_outputs() {
    let state = empty_state();
    let context = state.shielded_mint_context(&[2; 32]).unwrap();
    let address_material = material();
    assert_eq!(
        serializable_hex(&context.coin_public_key).unwrap(),
        address_material.shielded_coin_public_key_hex
    );
    assert_eq!(
        serializable_hex(&context.encryption_public_key).unwrap(),
        address_material.shielded_encryption_public_key_hex
    );
    assert_eq!(context.output_index, 0);

    let mut rng = StdRng::seed_from_u64(91);
    let coin = ShieldedCoinInfo::new(&mut rng, 5, ShieldedTokenType(HashOutput([7; 32])));
    let mut raw = Vec::new();
    tagged_serialize(&coin, &mut raw).unwrap();
    let (watched, output_index) = state.watch_shielded_mint(&raw, 0, &[2; 32]).unwrap();
    assert_eq!(output_index, 0);
    assert_eq!(watched.shielded.pending_outputs.iter().count(), 1);
    assert_eq!(state.shielded.pending_outputs.iter().count(), 0);

    assert!(matches!(
        state.watch_shielded_mint(&raw, 1, &[2; 32]),
        Err(MidnightRuntimeError::SyncGap)
    ));
    raw.push(0);
    assert!(matches!(
        state.watch_shielded_mint(&raw, 0, &[2; 32]),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
    assert_eq!(state.shielded.pending_outputs.iter().count(), 0);
}

#[test]
fn binary_sync_parsers_cover_valid_and_invalid_framing() {
    let mut cursor = BinaryCursor::new(&[1, 0, 0, 0, 9]);
    assert_eq!(cursor.u32_le().unwrap(), 1);
    assert_eq!(cursor.take(1).unwrap(), &[9]);
    assert_eq!(cursor.remaining(), 0);
    assert!(cursor.take(1).is_err());
    assert!(BinaryCursor::new(&[1]).u64_le().is_err());

    let mut spend = Vec::new();
    spend.extend_from_slice(&5_u64.to_le_bytes());
    spend.extend_from_slice(&0_u32.to_le_bytes());
    assert_eq!(dust_spend_records(&spend).unwrap().0, 5);
    assert!(dust_spend_records(&[]).is_err());
    let mut zero_event = Vec::new();
    zero_event.extend_from_slice(&0_u64.to_le_bytes());
    zero_event.extend_from_slice(&0_u32.to_le_bytes());
    assert!(dust_spend_records(&zero_event).is_err());

    let commitment = [0_u8; 8];
    assert!(inspect_dust_commitment_payload(&commitment).is_some());
    assert!(inspect_dust_commitment_payload(&[1, 0, 0, 0]).is_none());
    let mut response = commitment.to_vec();
    response.extend_from_slice(&7_u64.to_le_bytes());
    assert_eq!(
        dust_commitment_response_payload(&response, 7).unwrap(),
        commitment
    );
    assert!(matches!(
        dust_commitment_response_payload(&response, 8),
        Err(MidnightRuntimeError::SyncGap)
    ));

    for record_size in [
        DUST_SPEND_WASM_RECORD_BYTES,
        DUST_SPEND_EXTENDED_RECORD_BYTES,
    ] {
        let mut payload = Vec::new();
        payload.extend_from_slice(&11_u64.to_le_bytes());
        payload.extend_from_slice(&1_u32.to_le_bytes());
        let mut record = vec![0; record_size];
        record[..16].copy_from_slice(&[4; 16]);
        record[16..24].copy_from_slice(&3_u64.to_le_bytes());
        record[24..32].copy_from_slice(&7_u64.to_le_bytes());
        if record_size == DUST_SPEND_WASM_RECORD_BYTES {
            record[32..36].copy_from_slice(&9_u32.to_le_bytes());
        } else {
            record[40..48].copy_from_slice(&9_u64.to_le_bytes());
        }
        payload.extend(record);
        let (_, records) = dust_spend_records(&payload).unwrap();
        let parsed = records.get(&[4; 16]).unwrap();
        assert_eq!(parsed.commitment_index, 3);
        assert_eq!(parsed.v_fee, 7);
        assert_eq!(parsed.declared_time, 9);
    }

    let wire = WireUtxo {
        value: "12".to_owned(),
        owner: "owner".to_owned(),
        token_type: NIGHT.to_owned(),
        intent_hash: "01".repeat(32),
        output_index: 1,
        ctime: None,
        registered_for_dust_generation: false,
    };
    assert_eq!(wire.into_legacy(Some(5)).unwrap().meta.ctime, 5);
    let invalid = WireUtxo {
        value: "01".to_owned(),
        owner: "owner".to_owned(),
        token_type: NIGHT.to_owned(),
        intent_hash: "01".repeat(32),
        output_index: 1,
        ctime: Some(1),
        registered_for_dust_generation: false,
    };
    assert!(invalid.into_legacy(None).is_err());
}

#[test]
fn wire_offsets_and_unshielded_updates_cover_success_failure_and_validation() {
    assert!(NativeWalletState::validate_wire_offsets("unshielded", &[], 0, 0).is_ok());
    let first = serde_json::to_vec(&serde_json::json!({"id": 1})).unwrap();
    let second = serde_json::to_vec(&serde_json::json!({"id": 2})).unwrap();
    assert!(
        NativeWalletState::validate_wire_offsets("shielded-wire", &[first.clone(), second], 0, 2)
            .is_ok()
    );
    assert!(NativeWalletState::validate_wire_offsets("shielded-wire", &[], 0, 0).is_err());
    assert!(
        NativeWalletState::validate_wire_offsets(
            "shielded-wire",
            std::slice::from_ref(&first),
            1,
            2,
        )
        .is_err()
    );
    assert!(
        NativeWalletState::validate_wire_offsets("shielded-wire", &[b"{}".to_vec()], 0, 1).is_err()
    );

    let mut state = empty_state();
    add_unshielded(&mut state, "20", NIGHT, 0);
    let coin = state.unshielded.available_utxos[0].clone();
    let spent = serde_json::json!({
        "value": coin.utxo.value,
        "owner": coin.utxo.owner,
        "tokenType": coin.utxo.type_,
        "intentHash": coin.utxo.intent_hash,
        "outputIndex": coin.utxo.output_no,
        "ctime": 1,
        "registeredForDustGeneration": false
    });
    let success = serde_json::json!({
        "type": "UnshieldedTransaction",
        "transaction": {
            "id": 2,
            "type": "UserTransaction",
            "block": {"timestamp": 2},
            "transactionResult": {"status": "SUCCESS"}
        },
        "createdUtxos": [],
        "spentUtxos": [spent.clone()]
    });
    state
        .apply_unshielded(&serde_json::to_vec(&success).unwrap())
        .unwrap();
    assert!(state.unshielded.available_utxos.is_empty());

    state.unshielded.pending_utxos.push(coin);
    let failure = serde_json::json!({
        "type": "UnshieldedTransaction",
        "transaction": {
            "id": 3,
            "type": "UserTransaction",
            "transactionResult": {"status": "FAILURE"}
        },
        "createdUtxos": [],
        "spentUtxos": [spent]
    });
    state
        .apply_unshielded(&serde_json::to_vec(&failure).unwrap())
        .unwrap();
    assert_eq!(state.unshielded.available_utxos.len(), 1);
    assert!(state.apply_unshielded(b"{}").is_err());
}

#[test]
fn funded_wallet_builds_real_transfer_and_dapp_transactions() {
    let (state, keys, shielded_target) = funded_state();
    let mut rng = StdRng::seed_from_u64(21);
    let shielded_token = "08".repeat(32);
    let unshielded_token = "09".repeat(32);
    let target = unshielded_address("preview");

    let (shielded_next, shielded_raw) = state
        .build_shielded_transfer_with_rng(
            "preview",
            &shielded_target,
            100,
            &shielded_token,
            &[2; 32],
            &mut rng,
        )
        .unwrap();
    assert!(!shielded_raw.is_empty());
    assert!(!shielded_next.shielded.pending_spends.is_empty());

    let (unshielded_next, unshielded_raw) = state
        .build_unshielded_transfer_with_rng(
            UnshieldedTransferInput {
                network_id: "preview",
                target_address: &target,
                amount: 100,
                token_type: &unshielded_token,
                night_external_key: &[1; 32],
                ttl_seconds: 10_000,
            },
            &mut rng,
        )
        .unwrap();
    assert!(!unshielded_raw.is_empty());
    assert!(!unshielded_next.unshielded.pending_utxos.is_empty());

    let outputs = vec![
        DappTransactionOutput {
            wallet_type: "shielded".to_owned(),
            token_type: shielded_token.clone(),
            amount: 25,
            receiver_address: shielded_target,
        },
        DappTransactionOutput {
            wallet_type: "unshielded".to_owned(),
            token_type: unshielded_token.clone(),
            amount: 25,
            receiver_address: target,
        },
    ];
    assert!(
        state
            .build_dapp_transfer_with_rng(
                "preview",
                &outputs,
                &[1; 32],
                &[2; 32],
                10_000,
                &mut rng,
            )
            .is_ok()
    );
    let inputs = vec![
        DappIntentInput {
            wallet_type: "shielded".to_owned(),
            token_type: shielded_token,
            amount: 10,
        },
        DappIntentInput {
            wallet_type: "unshielded".to_owned(),
            token_type: unshielded_token,
            amount: 10,
        },
    ];
    assert!(
        state
            .build_dapp_intent_with_rng(
                DappIntentBuildInput {
                    network_id: "preview",
                    inputs: &inputs,
                    outputs: &outputs,
                    night_external_key: &[1; 32],
                    zswap_seed: &[2; 32],
                    ttl_seconds: 10_000,
                },
                &mut rng,
            )
            .is_ok()
    );
    assert!(decode_shielded_address(&shielded_address(&keys, "preview"), "preview").is_ok());

    assert!(
        state
            .build_shielded_transfer(
                "preview",
                &shielded_address(&keys, "preview"),
                10,
                &"08".repeat(32),
                &[2; 32],
            )
            .is_ok()
    );
    assert!(
        state
            .build_unshielded_transfer(
                "preview",
                &unshielded_address("preview"),
                10,
                &"09".repeat(32),
                &[1; 32],
                10_000,
            )
            .is_ok()
    );
    let (_, registration) = state
        .build_dust_registration_with_rng("preview", &[1; 32], &[3; 32], 10_000, 20_000, &mut rng)
        .unwrap();
    assert!(!registration.is_empty());
    assert!(
        state
            .validate_dust_registration_fee(&registration, &INITIAL_PARAMETERS, 0, 0)
            .is_ok()
    );
    assert!(
        state
            .build_dust_registration("preview", &[1; 32], &[3; 32], 10_000, 20_000,)
            .is_ok()
    );

    let empty = Transaction::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
        "preview".to_owned(),
        LedgerHashMap::new(),
        None,
        LedgerHashMap::new(),
    )
    .erase_proofs();
    assert!(
        state
            .build_dust_balance(DustBalanceInput {
                network_id: "preview",
                original: &empty,
                ledger_parameters: &INITIAL_PARAMETERS,
                fee_blocks_margin: 0,
                additional_fee_overhead: 0,
                dust_seed: &[3; 32],
                current_time_seconds: 10_000,
                ttl_seconds: 20_000,
            })
            .is_err()
    );
}
