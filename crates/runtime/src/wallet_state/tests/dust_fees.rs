use midnight_base_crypto::time::Timestamp;
use midnight_ledger::dust::{InitialNonce, dust_first_nonce};
use midnight_ledger::structure::{INITIAL_PARAMETERS, ProofMarker};
use midnight_transient_crypto::commitment::PureGeneratorPedersen;
use serde_json::Value;

use super::*;

#[test]
fn dust_registration_splits_night_by_generated_value_and_signs_one_intent() {
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
    let night_token = "00".repeat(32);
    for (output_no, value, ctime) in [
        (1_u32, 8_000_000_000_000_000_u128, 1_500_i64),
        (2_u32, 3_000_000_000_000_000_u128, 9_000_i64),
    ] {
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
                    ctime,
                    registered_for_dust_generation: false,
                },
            });
    }

    let mut rng = StdRng::from_seed([11_u8; 32]);
    let (proposed, raw) = state
        .build_dust_registration_with_rng("preprod", &night, &dust_seed, 100, 1_900, &mut rng)
        .unwrap();
    let tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&raw[..]).unwrap();
    let Transaction::Standard(tx) = tx else {
        unreachable!("registration must be a standard transaction")
    };
    let intent = tx.intents.get(&1).unwrap();
    let guaranteed = intent.guaranteed_unshielded_offer.as_ref().unwrap();
    let fallible = intent.fallible_unshielded_offer.as_ref().unwrap();
    assert_eq!(guaranteed.inputs.iter().count(), 1);
    assert_eq!(guaranteed.inputs.iter().next().unwrap().output_no, 1);
    assert_eq!(fallible.inputs.iter().count(), 1);
    assert_eq!(fallible.inputs.iter().next().unwrap().output_no, 2);
    assert_eq!(
        guaranteed.outputs.iter().next().unwrap().value,
        8_000_000_000_000_000
    );
    assert_eq!(
        fallible.outputs.iter().next().unwrap().value,
        3_000_000_000_000_000
    );

    let actions = intent.dust_actions.as_ref().unwrap();
    assert!(actions.spends.is_empty());
    let registration = actions.registrations.iter().next().unwrap();
    assert!(registration.allow_fee_payment > 0);
    assert_eq!(
        registration.allow_fee_payment,
        98_u128
            .saturating_mul(8_000_000_000_000_000)
            .saturating_mul(state.dust.params.generation_decay_rate as u128)
            .min(
                8_000_000_000_000_000_u128
                    .saturating_mul(state.dust.params.night_dust_ratio as u128),
            )
    );
    state
        .validate_dust_registration_fee(&raw, &INITIAL_PARAMETERS, 5, 300_000_000_000_000)
        .unwrap();
    assert!(matches!(
        state.validate_dust_registration_fee(
            &raw,
            &INITIAL_PARAMETERS,
            5,
            registration.allow_fee_payment,
        ),
        Err(MidnightRuntimeError::InsufficientDust)
    ));
    let expected_dust = DustPublicKey::from(DustSecretKey::derive_secret_key(
        &<[u8; 32]>::try_from(dust_seed.as_slice()).unwrap(),
    ));
    assert_eq!(**registration.dust_address.as_ref().unwrap(), expected_dust);
    let signature = registration.signature.as_ref().unwrap();
    let data_to_sign = intent.erase_proofs().erase_signatures().data_to_sign(1);
    assert!(crate::compat::verify(
        &SigningKey::from_bytes(&night).unwrap(),
        &data_to_sign,
        signature
    ));
    assert_eq!(guaranteed.signatures.iter().next().unwrap(), *signature);
    assert_eq!(fallible.signatures.iter().next().unwrap(), *signature);

    assert_eq!(state.unshielded.available_utxos.len(), 2);
    assert!(state.unshielded.pending_utxos.is_empty());
    assert!(proposed.unshielded.available_utxos.is_empty());
    assert_eq!(proposed.unshielded.pending_utxos.len(), 2);
}

#[test]
#[cfg(not(feature = "ledger-v9"))]
fn local_dust_balance_selects_smallest_outputs_and_keeps_state_proposed() {
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
    let now_seconds = 1_900_000_000_u64;
    let now = Timestamp::from_secs(now_seconds);
    for (index, initial_value) in [(0_u64, 1_u128), (1, u128::MAX / 8)] {
        let mut nonce_bytes = [0_u8; 32];
        nonce_bytes[0] = index as u8 + 1;
        let backing_night = InitialNonce(HashOutput(nonce_bytes));
        let generation_value = initial_value
            .checked_div(INITIAL_PARAMETERS.dust.night_dust_ratio as u128)
            .unwrap_or(0)
            .saturating_add(1);
        let generation = DustGenerationInfo {
            value: generation_value,
            owner,
            nonce: backing_night,
            dtime: Timestamp::from_secs(now_seconds + 60 * 60),
        };
        state.dust = state
            .dust
            .insert_generation_info(index, generation, Some(backing_night))
            .unwrap();
        let output = QualifiedDustOutput {
            initial_value,
            owner,
            nonce: dust_first_nonce(&backing_night, &owner),
            seq: 0,
            ctime: now,
            backing_night,
            mt_index: index,
        };
        state.dust = state.dust.insert_commitment(index, output, true).unwrap();
        state.dust = state
            .dust
            .add_utxo(&output.nullifier(&key), &output, None)
            .unwrap();
    }

    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/transaction-8.1.0.json")).unwrap();
    let original_raw = hex::decode(fixture["finalizedRawHex"].as_str().unwrap()).unwrap();
    let original: Transaction<Signature, ProofMarker, PureGeneratorPedersen, InMemoryDB> =
        tagged_deserialize(&original_raw[..]).unwrap();
    let erased = original.erase_proofs();
    let (proposed, balancing_raw) = state
        .build_dust_balance(DustBalanceInput {
            network_id: "preprod",
            original: &erased,
            ledger_parameters: &INITIAL_PARAMETERS,
            fee_blocks_margin: 5,
            additional_fee_overhead: 10,
            dust_seed: &dust_seed,
            current_time_seconds: now_seconds,
            ttl_seconds: now_seconds + 1_800,
        })
        .unwrap();
    let balancing_raw = balancing_raw.expect("fee payment requires DUST spends");
    let balancing: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&balancing_raw[..]).unwrap();
    let Transaction::Standard(standard) = &balancing else {
        unreachable!("balancing transaction must be standard")
    };
    let intent = standard.intents.get(&1).unwrap();
    let actions = intent.dust_actions.as_ref().unwrap();
    assert_eq!(actions.spends.iter().count(), 2);
    // The declared ctime selects roots from a block this wallet has already applied.
    assert_eq!(actions.ctime, Timestamp::from_secs(now_seconds - 30));

    let merged = erased.merge(&balancing.erase_proofs()).unwrap();
    let fee = merged.fees_with_margin(&INITIAL_PARAMETERS, 5).unwrap() + 10;
    assert!(
        merged
            .balance(Some(fee))
            .unwrap()
            .values()
            .all(|value| *value == 0)
    );
    assert_eq!(state.dust.utxos().count(), 2);
    assert_eq!(proposed.dust.utxos().count(), 0);

    // The coordinator pauses once per proof and accepts only tagged proof bytes.
    // It then seals and merges the original transaction.
    let mocked = balancing.mock_prove().unwrap();
    let Transaction::Standard(mocked_standard) = mocked else {
        unreachable!("mocked balancing transaction must be standard")
    };
    let mocked_intent = mocked_standard.intents.get(&1).unwrap();
    let mocked_proof = &mocked_intent
        .dust_actions
        .as_ref()
        .unwrap()
        .spends
        .iter()
        .next()
        .unwrap()
        .proof;
    let mut proof_bytes = Vec::new();
    tagged_serialize(mocked_proof, &mut proof_bytes).unwrap();
    let mut responses = crate::transaction::RemoteProofResponses::default();
    let finalized = (0..8)
        .find_map(|_| {
            match crate::transaction::advance_dust_balance(
                &original_raw,
                true,
                &balancing_raw,
                "preprod",
                &responses,
            )
            .unwrap()
            {
                crate::transaction::BalanceProgress::Network(requests) => {
                    for request in requests {
                        responses.accept(&request, proof_bytes.clone()).unwrap();
                    }
                    None
                }
                crate::transaction::BalanceProgress::Complete(finalized) => Some(finalized),
            }
        })
        .expect("all captured DUST proofs should complete");
    assert_eq!(
        crate::transaction::validate_finalized_transaction(&finalized.canonical, "preprod")
            .unwrap()
            .transaction_hash,
        finalized.transaction_hash
    );

    let declared_ctime = |state: &NativeWalletState, at: u64| {
        let raw = state
            .build_dust_balance(DustBalanceInput {
                network_id: "preprod",
                original: &erased,
                ledger_parameters: &INITIAL_PARAMETERS,
                fee_blocks_margin: 5,
                additional_fee_overhead: 10,
                dust_seed: &dust_seed,
                current_time_seconds: at,
                ttl_seconds: at + 1_800,
            })
            .unwrap()
            .1
            .expect("fee payment requires DUST spends");
        let decoded: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
            tagged_deserialize(&raw[..]).unwrap();
        let Transaction::Standard(standard) = &decoded else {
            unreachable!("balancing transaction must be standard")
        };
        standard
            .intents
            .get(&1)
            .unwrap()
            .dust_actions
            .as_ref()
            .unwrap()
            .ctime
    };
    // Backdating cannot precede the wallet's oldest available roots.
    state.dust.sync_time = Timestamp::from_secs(now_seconds - 5);
    assert_eq!(
        declared_ctime(&state, now_seconds),
        Timestamp::from_secs(now_seconds - 5),
    );

    // An older sync time leaves the lag value unchanged.
    state.dust.sync_time = Timestamp::from_secs(now_seconds - 600);
    assert_eq!(
        declared_ctime(&state, now_seconds),
        Timestamp::from_secs(now_seconds - 30),
    );
}
