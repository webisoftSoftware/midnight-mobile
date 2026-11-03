use super::*;

fn unshielded_address(address: &UserAddress, network_id: &str) -> String {
    let bytes = hex::decode(serializable_hex(address).unwrap()).unwrap();
    bech32::encode::<Bech32m>(
        Hrp::parse(&format!("mn_addr_{network_id}")).unwrap(),
        &bytes,
    )
    .unwrap()
}

#[test]
fn shielded_transfer_selects_smallest_coins_and_commits_pending_state_atomically() {
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
    let wallet_seed: [u8; 32] = secret("zswapSeedHex").try_into().unwrap();
    let wallet_keys = ZswapSecretKeys::from(ZswapSeed::from(wallet_seed));
    for (nonce, value) in [(1_u8, 8_u128), (2_u8, 3_u128)] {
        let mut nonce_bytes = [0_u8; 32];
        nonce_bytes[0] = nonce;
        state.shielded = state
            .shielded
            .insert_coin(
                &wallet_keys,
                ShieldedCoinInfo {
                    nonce: midnight_coin_structure::coin::Nonce(HashOutput(nonce_bytes)),
                    type_: ShieldedTokenType(HashOutput::default()),
                    value,
                },
            )
            .unwrap();
    }
    state.refresh_coin_hashes(&wallet_keys).unwrap();

    let target_keys = ZswapSecretKeys::from(ZswapSeed::from([9_u8; 32]));
    let target = shielded_address(&target_keys, "preprod");
    let mut rng = StdRng::from_seed([7_u8; 32]);
    let (proposed, raw) = state
        .build_shielded_transfer_with_rng(
            "preprod",
            &target,
            4,
            &"00".repeat(32),
            &wallet_seed,
            &mut rng,
        )
        .unwrap();
    let tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&raw[..]).unwrap();
    let Transaction::Standard(tx) = tx else {
        unreachable!("transfer must be a standard transaction")
    };
    let offer = tx.guaranteed_coins.as_ref().unwrap();
    assert_eq!(offer.inputs.iter().count(), 2);
    assert_eq!(offer.outputs.iter().count(), 2);
    assert!(tx.balance(None).unwrap().values().all(|value| *value == 0));

    let progress = crate::transaction::advance_unproven_transaction(
        &raw,
        "preprod",
        &crate::transaction::RemoteProofResponses::default(),
    )
    .unwrap();
    let crate::transaction::BalanceProgress::Network(requests) = progress else {
        unreachable!("a shielded transfer must pause for proofs")
    };
    assert!(
        requests.len() >= 2,
        "independent shielded proofs should be surfaced in one batch"
    );
    let mut request_keys = requests
        .iter()
        .map(|request| request.key.clone())
        .collect::<Vec<_>>();
    let captured = request_keys.len();
    request_keys.sort_unstable();
    request_keys.dedup();
    assert_eq!(
        request_keys.len(),
        captured,
        "batch requests must be unique"
    );

    assert_eq!(state.shielded.pending_spends.iter().count(), 0);
    assert_eq!(proposed.shielded.pending_spends.iter().count(), 2);
    assert_eq!(proposed.shielded.pending_outputs.iter().count(), 1);
    assert_eq!(state.balances().unwrap().total_shielded, "11");
    assert_eq!(proposed.balances().unwrap().total_shielded, "7");
    assert_eq!(proposed.coin_hashes.len(), 3);

    let mut insufficient_rng = StdRng::from_seed([8_u8; 32]);
    assert!(matches!(
        state.build_shielded_transfer_with_rng(
            "preprod",
            &target,
            12,
            &"00".repeat(32),
            &wallet_seed,
            &mut insufficient_rng,
        ),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
    assert_eq!(state.shielded.pending_spends.iter().count(), 0);
}

#[test]
fn unshielded_transfer_signs_the_intent_and_reserves_selected_utxos_atomically() {
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
    let night_token = "00".repeat(32);
    for (output_no, value) in [(1_u32, 8_u128), (2_u32, 3_u128)] {
        state
            .unshielded
            .available_utxos
            .push(UnshieldedUtxoWithMeta {
                utxo: UnshieldedUtxo {
                    value: value.to_string(),
                    owner: material.unshielded_address_hex.clone(),
                    type_: night_token.clone(),
                    intent_hash: format!("{output_no:02x}").repeat(32),
                    output_no,
                },
                meta: UnshieldedMeta {
                    ctime: 1_000,
                    registered_for_dust_generation: false,
                },
            });
    }

    let target_key = SigningKey::from_bytes(&[9_u8; 32]).unwrap();
    let target_user_address = UserAddress::from(target_key.verifying_key());
    let target = unshielded_address(&target_user_address, "preprod");
    let mut rng = StdRng::from_seed([7_u8; 32]);
    let (proposed, raw) = state
        .build_unshielded_transfer_with_rng(
            UnshieldedTransferInput {
                network_id: "preprod",
                target_address: &target,
                amount: 4,
                token_type: &night_token,
                night_external_key: &night,
                ttl_seconds: 1_900_000_000,
            },
            &mut rng,
        )
        .unwrap();
    let tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&raw[..]).unwrap();
    let Transaction::Standard(tx) = tx else {
        unreachable!("transfer must be a standard transaction")
    };
    let intent = tx.intents.get(&1).unwrap();
    assert!(intent.guaranteed_unshielded_offer.is_none());
    let offer = intent.fallible_unshielded_offer.as_ref().unwrap();
    assert_eq!(offer.inputs.iter().count(), 2);
    assert_eq!(offer.outputs.iter().count(), 2);
    assert!(
        offer
            .outputs
            .iter()
            .any(|output| { output.value == 4 && output.owner == target_user_address })
    );
    let signatures = offer.signatures.iter().collect::<Vec<_>>();
    assert_eq!(signatures.len(), 2);
    assert_eq!(signatures[0], signatures[1]);
    let data_to_sign = intent.erase_proofs().erase_signatures().data_to_sign(1);
    assert!(crate::compat::verify(
        &SigningKey::from_bytes(&night).unwrap(),
        &data_to_sign,
        &signatures[0]
    ));

    assert_eq!(state.unshielded.available_utxos.len(), 2);
    assert!(state.unshielded.pending_utxos.is_empty());
    assert!(proposed.unshielded.available_utxos.is_empty());
    assert_eq!(proposed.unshielded.pending_utxos.len(), 2);
    assert_eq!(state.balances().unwrap().total_unshielded, "11");
    assert_eq!(proposed.balances().unwrap().total_unshielded, "11");

    let mut insufficient_rng = StdRng::from_seed([8_u8; 32]);
    assert!(matches!(
        state.build_unshielded_transfer_with_rng(
            UnshieldedTransferInput {
                network_id: "preprod",
                target_address: &target,
                amount: 12,
                token_type: &night_token,
                night_external_key: &night,
                ttl_seconds: 1_900_000_000,
            },
            &mut insufficient_rng,
        ),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
    assert!(state.unshielded.pending_utxos.is_empty());
}
