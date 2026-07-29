use midnight_ledger::events::{Event, EventDetails, EventSource, ZswapPreimageEvidence};
use midnight_ledger::structure::{INITIAL_PARAMETERS, TransactionHash};

use super::*;

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
