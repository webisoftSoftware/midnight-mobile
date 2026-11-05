use midnight_base_crypto::time::Timestamp;
use midnight_ledger::dust::{InitialNonce, dust_first_nonce};
use midnight_ledger::events::{Event, EventDetails, EventSource, ZswapPreimageEvidence};
use midnight_ledger::structure::{INITIAL_PARAMETERS, TransactionHash};

use serde_json::Value;

use super::*;

#[test]
fn snapshot_requests_and_v2_imports_are_native_and_integrity_checked() {
    let state = empty_state();
    let shielded_request: serde_json::Value = serde_json::from_slice(
        &NativeWalletState::snapshot_sync_request("shielded", &[2; 32], &[3; 32]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        shielded_request["coinPublicKey"].as_str().unwrap().len(),
        64
    );
    assert!(shielded_request["viewingKey"].as_str().is_some_and(
        |value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    ));
    let dust_request: serde_json::Value = serde_json::from_slice(
        &NativeWalletState::snapshot_sync_request("dust", &[2; 32], &[3; 32]).unwrap(),
    )
    .unwrap();
    assert_eq!(dust_request["dustPublicKey"].as_str().unwrap().len(), 64);
    assert!(NativeWalletState::snapshot_sync_request("unshielded", &[2; 32], &[3; 32]).is_err());

    let mut shielded = Vec::new();
    shielded.extend_from_slice(&0_u32.to_le_bytes());
    shielded.extend_from_slice(&0_u32.to_le_bytes());
    shielded.extend_from_slice(&17_u64.to_le_bytes());
    NativeWalletState::validate_v2_trailer("shielded-v2", std::slice::from_ref(&shielded), 17)
        .unwrap();
    assert!(
        NativeWalletState::validate_v2_trailer("shielded-v2", std::slice::from_ref(&shielded), 18,)
            .is_err()
    );
    let mut zero_tip = shielded.clone();
    let trailer = zero_tip.len() - 8;
    zero_tip[trailer..].copy_from_slice(&0_u64.to_le_bytes());
    assert!(
        NativeWalletState::validate_v2_trailer("shielded-v2", std::slice::from_ref(&zero_tip), 0,)
            .is_err()
    );
    let imported_shielded = state
        .apply_batch("shielded-v2", &[shielded], &[2; 32], &[3; 32])
        .unwrap();
    assert_eq!(imported_shielded.shielded.first_free, 0);

    let mut dust = Vec::new();
    dust.extend_from_slice(&0_u32.to_le_bytes());
    dust.extend_from_slice(&1_700_000_000_u64.to_le_bytes());
    dust.extend_from_slice(&0_u32.to_le_bytes());
    dust.extend_from_slice(&0_u32.to_le_bytes());
    dust.extend_from_slice(&0_u32.to_le_bytes());
    dust.extend_from_slice(&0_u32.to_le_bytes());
    dust.extend_from_slice(&23_u64.to_le_bytes());
    NativeWalletState::validate_v2_trailer("dust-v2", std::slice::from_ref(&dust), 23).unwrap();
    let imported_dust = state
        .apply_batch("dust-v2", &[dust], &[2; 32], &[3; 32])
        .unwrap();
    assert_eq!(imported_dust.dust.generating_tree_first_free, 0);
    assert_eq!(imported_dust.dust.commitment_tree_first_free, 0);
}

#[test]
fn v2_dust_import_reads_tagged_dust_parameters() {
    let state = empty_state();

    // The gateway sends `midnight:dust-parameters[v1]:` + payload. Reading that
    // as an untagged struct does not fail — it parses the tag bytes as the ratio
    // and decay rate, so the import succeeds with parameters that undervalue
    // every coin by orders of magnitude.
    let mut tagged = Vec::new();
    tagged_serialize(&INITIAL_DUST_PARAMETERS, &mut tagged).unwrap();

    let mut payload = Vec::new();
    payload.extend_from_slice(&u32::try_from(tagged.len()).unwrap().to_le_bytes());
    payload.extend_from_slice(&tagged);
    payload.extend_from_slice(&1_700_000_000_u64.to_le_bytes()); // syncTime
    payload.extend_from_slice(&0_u32.to_le_bytes()); // generation count
    payload.extend_from_slice(&0_u32.to_le_bytes()); // generation tail
    payload.extend_from_slice(&0_u32.to_le_bytes()); // commitment count
    payload.extend_from_slice(&0_u32.to_le_bytes()); // commitment tail
    payload.extend_from_slice(&23_u64.to_le_bytes()); // event trailer

    let imported = state
        .apply_batch("dust-v2", &[payload], &[2; 32], &[3; 32])
        .unwrap();
    assert_eq!(imported.dust.params, INITIAL_DUST_PARAMETERS);
}

#[test]
fn wallet_sync_snapshot_restore_and_dust_requests_round_trip() {
    let (state, keys, _) = funded_state();
    let balances = state.balances().unwrap();
    assert_eq!(
        balances.unshielded_balances.get(NIGHT).unwrap(),
        "1000000000000"
    );
    assert_eq!(
        balances.shielded_balances.get(&"08".repeat(32)).unwrap(),
        "700"
    );

    let address_material = material();
    let legacy = state
        .export_legacy(LegacyExportContext {
            network_id: "preview",
            unshielded_address: "synthetic-address",
            address_material: &address_material,
            night_external_key: &[1; 32],
            shielded_offset: Some(3),
            dust_offset: Some(4),
            unshielded_offset: Some(5),
        })
        .unwrap();
    let restored = NativeWalletState::restore(
        &legacy,
        RestoreContext {
            network_id: "preview",
            unshielded_address: "synthetic-address",
            address_material: &address_material,
            night_external_key: &[1; 32],
        },
    )
    .unwrap();
    assert_eq!(restored.balances().unwrap(), balances);

    assert_eq!(state.shielded_spent_request().unwrap(), {
        let prefixes = state
            .coin_hashes
            .values()
            .map(|hashes| hashes.nullifier[..32].to_owned())
            .collect::<Vec<_>>();
        serde_json::to_vec(&serde_json::json!({"nullifierPrefixes": prefixes})).unwrap()
    });
    let (unchanged, removed) = state
        .apply_shielded_spent_response(br#"{"results":[]}"#, &[2; 32])
        .unwrap();
    assert_eq!(removed, 0);
    assert_eq!(unchanged.balances().unwrap(), balances);
    let nullifier = state.coin_hashes.values().next().unwrap().nullifier.clone();
    let (spent, removed) = state
        .apply_shielded_spent_response(
            &serde_json::to_vec(&serde_json::json!({
                "results": [nullifier.to_uppercase(), {"nullifier": nullifier}]
            }))
            .unwrap(),
            &[2; 32],
        )
        .unwrap();
    assert_eq!(removed, 1);
    assert!(spent.shielded.coins.iter().next().is_none());
    assert!(
        state
            .apply_shielded_spent_response(br#"{"results":["bad"]}"#, &[2; 32])
            .is_err()
    );
    assert!(state.set_shielded_protocol_version(0).is_err());
    assert_eq!(
        state
            .set_shielded_protocol_version(9)
            .unwrap()
            .protocol_version,
        "9"
    );
    assert_eq!(state.dust_spend_request(&[3; 32]).unwrap().1, 0);

    let mut ahead = Vec::new();
    ahead.extend_from_slice(&9_u64.to_le_bytes());
    ahead.extend_from_slice(&0_u32.to_le_bytes());
    assert!(matches!(
        state.dust_commitment_request(&ahead, 8, &[3; 32]).unwrap(),
        DustCommitmentRequest::Ahead
    ));
    assert!(matches!(
        state.dust_commitment_request(&ahead, 9, &[3; 32]).unwrap(),
        DustCommitmentRequest::Unchanged
    ));

    let mut rng = StdRng::seed_from_u64(31);
    let coin = ShieldedCoinInfo::new(&mut rng, 5, ShieldedTokenType(HashOutput([7; 32])));
    let event: Event<InMemoryDB> = Event {
        source: EventSource {
            transaction_hash: TransactionHash(HashOutput::default()),
            logical_segment: 0,
            physical_segment: 0,
        },
        content: EventDetails::ZswapOutput {
            commitment: coin.commitment(&Recipient::User(keys.coin_public_key())),
            preimage_evidence: ZswapPreimageEvidence::PublicPreimage {
                coin,
                recipient: Recipient::User(keys.coin_public_key()),
            },
            contract: None,
            mt_index: 0,
        },
    };
    let mut raw = Vec::new();
    tagged_serialize(&event, &mut raw).unwrap();
    assert!(
        empty_state()
            .apply_batch("shielded", &[raw.clone()], &[2; 32], &[3; 32])
            .is_ok()
    );
    let wire = serde_json::to_vec(&serde_json::json!({
        "id": 1,
        "raw": hex::encode(&raw),
        "protocolVersion": 8
    }))
    .unwrap();
    assert!(
        empty_state()
            .apply_batch("shielded-wire", &[wire], &[2; 32], &[3; 32])
            .is_ok()
    );
    assert!(
        empty_state()
            .apply_batch("dust", &[raw.clone()], &[2; 32], &[3; 32])
            .is_ok()
    );
    let dust_wire = serde_json::to_vec(&serde_json::json!({
        "id": 1,
        "raw": hex::encode(&raw)
    }))
    .unwrap();
    assert!(
        empty_state()
            .apply_batch("dust-wire", &[dust_wire], &[2; 32], &[3; 32])
            .is_ok()
    );
    assert!(
        empty_state()
            .apply_batch("shielded-wire", &[b"{}".to_vec()], &[2; 32], &[3; 32])
            .is_err()
    );
    assert!(
        empty_state()
            .apply_batch("unknown", &[], &[2; 32], &[3; 32])
            .is_err()
    );
    assert!(
        state
            .validate_dust_registration_fee(&[], &INITIAL_PARAMETERS, 0, 0)
            .is_err()
    );
}

#[test]
fn nullable_unshielded_ctime_uses_the_transaction_block_time() {
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
    let update = |registered| {
        serde_json::json!({
            "type": "UnshieldedTransaction",
            "transaction": {
                "id": 1,
                "type": "RegularTransaction",
                "block": { "timestamp": 1_700_000_123_456_i64 },
                "transactionResult": { "status": "SUCCESS" }
            },
            "createdUtxos": [{
                "value": "7",
                "owner": "mn_addr_preprod1fixture",
                "tokenType": "00".repeat(32),
                "intentHash": "11".repeat(32),
                "outputIndex": 0,
                "ctime": null,
                "registeredForDustGeneration": registered
            }],
            "spentUtxos": []
        })
        .to_string()
        .into_bytes()
    };
    let applied = state
        .apply_batch(
            "unshielded",
            &[update(false)],
            &secret("zswapSeedHex"),
            &secret("dustSeedHex"),
        )
        .unwrap();
    assert_eq!(
        applied.unshielded.available_utxos[0].meta.ctime,
        1_700_000_123_456
    );
    assert!(matches!(
        state.apply_batch(
            "unshielded",
            &[update(true)],
            &secret("zswapSeedHex"),
            &secret("dustSeedHex"),
        ),
        Err(MidnightRuntimeError::InvalidArgument)
    ));

    let mut without_block_time: Value = serde_json::from_slice(&update(false)).unwrap();
    without_block_time["transaction"]
        .as_object_mut()
        .unwrap()
        .remove("block");
    assert!(matches!(
        state.apply_batch(
            "unshielded",
            &[serde_json::to_vec(&without_block_time).unwrap()],
            &secret("zswapSeedHex"),
            &secret("dustSeedHex"),
        ),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
}

#[test]
fn shielded_spent_reconciliation_only_removes_exact_owned_nullifiers() {
    let material = fixture_material();
    let night = secret("nightExternalKeyHex");
    let zswap_seed = secret("zswapSeedHex");
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
    let seed: [u8; 32] = zswap_seed.clone().try_into().unwrap();
    let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
    let coin = ShieldedCoinInfo {
        nonce: Default::default(),
        type_: Default::default(),
        value: 7,
    };
    state.shielded = state.shielded.insert_coin(&keys, coin).unwrap();
    state.refresh_coin_hashes(&keys).unwrap();
    let nullifier = state.coin_hashes.values().next().unwrap().nullifier.clone();
    let request: Value = serde_json::from_slice(&state.shielded_spent_request().unwrap()).unwrap();
    assert_eq!(request["nullifierPrefixes"][0], &nullifier[..32]);

    let response = serde_json::json!({
        "results": [
            { "nullifier": "ff".repeat(32), "eventId": 16, "spent": true },
            {
                "nullifier": nullifier.to_ascii_uppercase(),
                "eventId": 17,
                "spent": true
            },
        ],
        "lastEventId": 17,
    })
    .to_string();
    let (proposed, removed) = state
        .apply_shielded_spent_response(response.as_bytes(), &zswap_seed)
        .unwrap();
    assert_eq!(removed, 1);
    assert_eq!(state.shielded.coins.iter().count(), 1);
    assert_eq!(proposed.shielded.coins.iter().count(), 0);
    assert!(proposed.coin_hashes.is_empty());

    assert!(matches!(
        state.apply_shielded_spent_response(br#"{"results":[{"nullifier":"ABC"}]}"#, &zswap_seed,),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
}

#[test]
fn dust_spend_chain_resolution_matches_the_established_36_byte_identity_layout() {
    let material = fixture_material();
    let night = secret("nightExternalKeyHex");
    let dust_seed = secret("dustSeedHex");
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
    let seed: [u8; 32] = dust_seed.clone().try_into().unwrap();
    let key = DustSecretKey::derive_secret_key(&seed);
    let owner = DustPublicKey::from(key.clone());
    let backing_night = InitialNonce(HashOutput([7_u8; 32]));
    let generation = DustGenerationInfo {
        value: 1_000,
        owner,
        nonce: backing_night,
        dtime: Timestamp::from_secs(2_000_000_000),
    };
    state.dust = state
        .dust
        .insert_generation_info(0, generation, Some(backing_night))
        .unwrap();
    let output = QualifiedDustOutput {
        initial_value: 100,
        owner,
        nonce: dust_first_nonce(&backing_night, &owner),
        seq: 0,
        ctime: Timestamp::from_secs(1_900_000_000),
        backing_night,
        mt_index: 0,
    };
    state.dust = state.dust.insert_commitment(0, output, true).unwrap();
    state.dust = state
        .dust
        .add_utxo(&output.nullifier(&key), &output, None)
        .unwrap();

    let mut spend = Vec::new();
    spend.extend_from_slice(&7_u64.to_le_bytes());
    spend.extend_from_slice(&1_u32.to_le_bytes());
    spend.extend_from_slice(&output.nullifier(&key).0.as_le_bytes()[..16]);
    spend.extend_from_slice(&9_u64.to_le_bytes());
    spend.extend_from_slice(&3_u64.to_le_bytes());
    spend.extend_from_slice(&1_900_000_010_u32.to_le_bytes());
    let DustSpendResolution::Changed(resolved) = state
        .resolve_dust_spend_chains(&spend, 7, &dust_seed)
        .unwrap()
    else {
        unreachable!("matching spend record must advance the wallet UTXO")
    };
    let successor = resolved.utxos().next().unwrap();
    assert_eq!(successor.seq, 1);
    assert_eq!(successor.mt_index, 9);
    assert_eq!(successor.ctime, Timestamp::from_secs(1_900_000_010));
    assert_eq!(state.dust.utxos().next().unwrap(), output);

    let mut duplicate = spend.clone();
    duplicate[8..12].copy_from_slice(&2_u32.to_le_bytes());
    duplicate.extend_from_slice(&spend[12..]);
    assert!(matches!(
        state.resolve_dust_spend_chains(&duplicate, 7, &dust_seed),
        Err(MidnightRuntimeError::InvalidArgument)
    ));

    let mut stale = spend;
    stale[..8].copy_from_slice(&6_u64.to_le_bytes());
    assert!(matches!(
        state.resolve_dust_spend_chains(&stale, 7, &dust_seed),
        Err(MidnightRuntimeError::SyncGap)
    ));
}

#[test]
fn dust_spend_and_commitment_rebuild_commit_as_one_native_state_transition() {
    let material = fixture_material();
    let night = secret("nightExternalKeyHex");
    let dust_seed = secret("dustSeedHex");
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
    let seed: [u8; 32] = dust_seed.clone().try_into().unwrap();
    let key = DustSecretKey::derive_secret_key(&seed);
    let owner = DustPublicKey::from(key.clone());
    let backing_night = InitialNonce(HashOutput([8_u8; 32]));
    let generation = DustGenerationInfo {
        value: 1_000,
        owner,
        nonce: backing_night,
        dtime: Timestamp::from_secs(2_000_000_000),
    };
    state.dust = state
        .dust
        .insert_generation_info(0, generation, Some(backing_night))
        .unwrap();
    let output = QualifiedDustOutput {
        initial_value: 100,
        owner,
        nonce: dust_first_nonce(&backing_night, &owner),
        seq: 0,
        ctime: Timestamp::from_secs(1_900_000_000),
        backing_night,
        mt_index: 0,
    };
    state.dust = state.dust.insert_commitment(0, output, true).unwrap();
    state.dust = state
        .dust
        .add_utxo(&output.nullifier(&key), &output, None)
        .unwrap();

    let mut spend = Vec::new();
    spend.extend_from_slice(&11_u64.to_le_bytes());
    spend.extend_from_slice(&1_u32.to_le_bytes());
    spend.extend_from_slice(&output.nullifier(&key).0.as_le_bytes()[..16]);
    spend.extend_from_slice(&0_u64.to_le_bytes());
    spend.extend_from_slice(&3_u64.to_le_bytes());
    spend.extend_from_slice(&1_900_000_010_u32.to_le_bytes());

    let mut placeholder = Vec::new();
    output.serialize(&mut placeholder).unwrap();
    let mut commitment = Vec::new();
    commitment.extend_from_slice(&1_u32.to_le_bytes());
    commitment.extend_from_slice(&0_u32.to_le_bytes());
    commitment.extend_from_slice(&(placeholder.len() as u32).to_le_bytes());
    commitment.extend_from_slice(&placeholder);
    commitment.extend_from_slice(&0_u64.to_le_bytes());
    commitment.extend_from_slice(&0_u32.to_le_bytes());
    commitment.extend_from_slice(&11_u64.to_le_bytes());

    let proposed = state
        .apply_dust_spend_resolution(&spend, &commitment, 11, &dust_seed)
        .unwrap();
    assert_eq!(state.dust.utxos().next().unwrap().seq, 0);
    assert_eq!(proposed.dust.utxos().next().unwrap().seq, 1);
    assert_eq!(proposed.dust.commitment_tree_first_free, 1);

    let mut wrong_tip = commitment;
    let tip_offset = wrong_tip.len() - 8;
    wrong_tip[tip_offset..].copy_from_slice(&12_u64.to_le_bytes());
    assert!(matches!(
        state.apply_dust_spend_resolution(&spend, &wrong_tip, 11, &dust_seed),
        Err(MidnightRuntimeError::SyncGap)
    ));
    assert_eq!(state.dust.utxos().next().unwrap().seq, 0);
}
