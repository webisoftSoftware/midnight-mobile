use midnight_coin_structure::coin::Commitment;
use midnight_ledger::events::{EventDetails, EventSource, ZswapPreimageEvidence};
use midnight_ledger::structure::TransactionHash;
use serde_json::Value;

use super::*;

#[test]
fn extended_dust_spend_records_require_exact_framing_and_keep_u64_time() {
    let declared_time = u64::from(u32::MAX) + 42;
    let mut payload = Vec::new();
    payload.extend_from_slice(&1_u64.to_le_bytes());
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&[7_u8; 16]);
    payload.extend_from_slice(&2_u64.to_le_bytes());
    payload.extend_from_slice(&3_u64.to_le_bytes());
    payload.extend_from_slice(&[0_u8; 8]);
    payload.extend_from_slice(&declared_time.to_le_bytes());

    let (_, records) = dust_spend_records(&payload).unwrap();
    assert_eq!(
        records.get(&[7_u8; 16]).unwrap().declared_time,
        declared_time
    );

    payload.push(0);
    assert!(matches!(
        dust_spend_records(&payload),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
}

#[test]
fn empty_native_states_match_legacy_and_round_trip_snapshots() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/wallet-state-8.1.0.json")).unwrap();
    let material = fixture_material();
    let night = secret("nightExternalKeyHex");
    let state = NativeWalletState::restore(
        &LegacyWalletState::default(),
        RestoreContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
        },
    )
    .unwrap();
    assert_eq!(
        encode_hex_state(&state.shielded).unwrap(),
        fixture["emptyShieldedStateHex"].as_str().unwrap()
    );
    assert_eq!(
        encode_hex_state(&state.dust).unwrap(),
        fixture["emptyDustStateHex"].as_str().unwrap()
    );

    let legacy = state
        .export_legacy(LegacyExportContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
            shielded_offset: Some(0),
            dust_offset: Some(0),
            unshielded_offset: Some(0),
        })
        .unwrap();
    assert_eq!(
        legacy.serialized_shielded_wallet_state,
        fixture["emptyShieldedSnapshot"].as_str().unwrap()
    );
    assert_eq!(
        legacy.serialized_dust_wallet_state,
        fixture["emptyDustSnapshot"].as_str().unwrap()
    );
    let restored = NativeWalletState::restore(
        &legacy,
        RestoreContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
        },
    )
    .unwrap();
    assert_eq!(restored.state_count(), 0);
    assert_eq!(
        encode_hex_state(&restored.shielded).unwrap(),
        fixture["emptyShieldedStateHex"].as_str().unwrap()
    );
}

#[test]
fn empty_unshielded_snapshot_is_byte_for_byte_legacy_compatible() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/wallet-state-8.1.0.json")).unwrap();
    let night = hex::decode(fixture["emptyUnshieldedSecretKeyHex"].as_str().unwrap()).unwrap();
    let material = WalletAddressMaterial {
        unshielded_address_hex: fixture["emptyUnshieldedAddressHex"]
            .as_str()
            .unwrap()
            .to_owned(),
        shielded_coin_public_key_hex: fixture_material().shielded_coin_public_key_hex,
        shielded_encryption_public_key_hex: fixture_material().shielded_encryption_public_key_hex,
        dust_public_key: fixture_material().dust_public_key,
    };
    let expected = fixture["emptyUnshieldedSnapshot"].as_str().unwrap();
    let state = NativeWalletState::restore(
        &LegacyWalletState {
            serialized_unshielded_wallet_state: expected.to_owned(),
            ..LegacyWalletState::default()
        },
        RestoreContext {
            network_id: "preview",
            unshielded_address: fixture["emptyUnshieldedAddress"].as_str().unwrap(),
            address_material: &material,
            night_external_key: &night,
        },
    )
    .unwrap();
    let exported = state
        .export_legacy(LegacyExportContext {
            network_id: "preview",
            unshielded_address: fixture["emptyUnshieldedAddress"].as_str().unwrap(),
            address_material: &material,
            night_external_key: &night,
            shielded_offset: None,
            dust_offset: None,
            unshielded_offset: Some(0),
        })
        .unwrap();
    assert_eq!(exported.serialized_unshielded_wallet_state, expected);
}

#[test]
fn unshielded_batches_apply_atomically_and_export_legacy_shape() {
    let material = fixture_material();
    let night = secret("nightExternalKeyHex");
    let state = NativeWalletState::restore(
        &LegacyWalletState::default(),
        RestoreContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
        },
    )
    .unwrap();
    let created = serde_json::json!({
        "type": "UnshieldedTransaction",
        "transaction": {
            "id": 1,
            "type": "RegularTransaction",
            "transactionResult": { "status": "SUCCESS" }
        },
        "createdUtxos": [{
            "value": "42",
            "owner": material.unshielded_address_hex,
            "tokenType": "02",
            "intentHash": "intent-1",
            "outputIndex": 0,
            "ctime": 123,
            "registeredForDustGeneration": false
        }],
        "spentUtxos": []
    })
    .to_string()
    .into_bytes();
    let proposed = state
        .apply_batch(
            "unshielded",
            &[created],
            &secret("zswapSeedHex"),
            &secret("dustSeedHex"),
        )
        .unwrap();
    assert_eq!(state.state_count(), 0);
    assert_eq!(proposed.state_count(), 1);
    let balances = proposed.balances().unwrap();
    assert_eq!(balances.total_unshielded_all, "42");
    assert_eq!(balances.total_unshielded, "0");
    let legacy = proposed
        .export_legacy(LegacyExportContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
            shielded_offset: None,
            dust_offset: None,
            unshielded_offset: Some(1),
        })
        .unwrap();
    let snapshot: Value = serde_json::from_str(&legacy.serialized_unshielded_wallet_state).unwrap();
    assert_eq!(
        snapshot["state"]["availableUtxos"][0]["utxo"]["value"],
        "42"
    );
    assert_eq!(
        snapshot["state"]["availableUtxos"][0]["meta"]["ctime"],
        123_000
    );
}

#[test]
fn shielded_raw_event_sequences_are_decoded_and_committed_atomically() {
    let material = fixture_material();
    let night = secret("nightExternalKeyHex");
    let state = NativeWalletState::restore(
        &LegacyWalletState::default(),
        RestoreContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
        },
    )
    .unwrap();
    let event: Event<InMemoryDB> = Event {
        source: EventSource {
            transaction_hash: TransactionHash::default(),
            logical_segment: 0,
            physical_segment: 0,
        },
        content: EventDetails::ZswapOutput {
            commitment: Commitment::default(),
            preimage_evidence: ZswapPreimageEvidence::None,
            contract: None,
            mt_index: 0,
        },
    };
    let mut raw = Vec::new();
    tagged_serialize(&event, &mut raw).unwrap();
    let wire = |id, protocol_version| {
        serde_json::json!({
            "id": id,
            "raw": hex::encode(&raw),
            "protocolVersion": protocol_version,
        })
        .to_string()
        .into_bytes()
    };
    assert!(matches!(
        state.apply_batch(
            "shielded-wire",
            &[wire(1, 1), wire(2, 2)],
            &secret("zswapSeedHex"),
            &secret("dustSeedHex"),
        ),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
    let proposed = state
        .apply_batch(
            "shielded",
            &[raw],
            &secret("zswapSeedHex"),
            &secret("dustSeedHex"),
        )
        .unwrap();
    assert_eq!(state.shielded.first_free, 0);
    assert_eq!(proposed.shielded.first_free, 1);

    assert_eq!(
        state
            .apply_batch(
                "shielded",
                &[b"malformed".to_vec()],
                &secret("zswapSeedHex"),
                &secret("dustSeedHex"),
            )
            .unwrap_err()
            .to_string(),
        "INVALID_ARGUMENT"
    );
    assert_eq!(state.shielded.first_free, 0);
}

#[test]
fn shielded_coin_hashes_are_refreshed_and_required_for_legacy_restore() {
    let material = fixture_material();
    let night = secret("nightExternalKeyHex");
    let mut state = NativeWalletState::restore(
        &LegacyWalletState::default(),
        RestoreContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
        },
    )
    .unwrap();
    let seed: [u8; 32] = secret("zswapSeedHex").try_into().unwrap();
    let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
    let coin = ShieldedCoinInfo {
        nonce: Default::default(),
        type_: Default::default(),
        value: 7,
    };
    state.shielded = state.shielded.insert_coin(&keys, coin).unwrap();
    state.refresh_coin_hashes(&keys).unwrap();

    let nonce = serializable_hex(&coin.nonce).unwrap();
    let hashes = state.coin_hashes.get(&nonce).unwrap();
    assert_eq!(
        hashes.commitment,
        serializable_hex(&coin.commitment(&Recipient::User(keys.coin_public_key()))).unwrap()
    );
    assert_eq!(
        hashes.nullifier,
        serializable_hex(
            &coin.nullifier(&SenderEvidence::User(Cow::Borrowed(&keys.coin_secret_key,)))
        )
        .unwrap()
    );

    let legacy = state
        .export_legacy(LegacyExportContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
            shielded_offset: Some(1),
            dust_offset: None,
            unshielded_offset: None,
        })
        .unwrap();
    NativeWalletState::restore(
        &legacy,
        RestoreContext {
            network_id: "preprod",
            unshielded_address: "mn_addr_preprod1fixture",
            address_material: &material,
            night_external_key: &night,
        },
    )
    .unwrap();

    let mut broken: ShieldedSnapshot =
        serde_json::from_str(&legacy.serialized_shielded_wallet_state).unwrap();
    broken.coin_hashes.clear();
    let broken = LegacyWalletState {
        serialized_shielded_wallet_state: serde_json::to_string(&broken).unwrap(),
        ..legacy
    };
    assert!(matches!(
        NativeWalletState::restore(
            &broken,
            RestoreContext {
                network_id: "preprod",
                unshielded_address: "mn_addr_preprod1fixture",
                address_material: &material,
                night_external_key: &night,
            },
        ),
        Err(MidnightRuntimeError::StateIncompatible)
    ));
}
