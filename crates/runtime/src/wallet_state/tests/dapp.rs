use midnight_base_crypto::schnorr::SigningKey;
use midnight_coin_structure::coin::UserAddress;
use midnight_ledger::structure::{ProofPreimageMarker, Transaction};
use midnight_serialize::tagged_deserialize;
use midnight_storage::db::InMemoryDB;
use midnight_transient_crypto::commitment::PedersenRandomness;
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::*;

fn state_with_unshielded(value: u128, token_type: &str) -> NativeWalletState {
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
    state
        .unshielded
        .available_utxos
        .push(UnshieldedUtxoWithMeta {
            utxo: UnshieldedUtxo {
                value: value.to_string(),
                owner: material.unshielded_address_hex,
                type_: token_type.to_owned(),
                intent_hash: "01".repeat(32),
                output_no: 1,
            },
            meta: UnshieldedMeta {
                ctime: 1_000,
                registered_for_dust_generation: false,
            },
        });
    state
}

fn target_address(seed: u8) -> String {
    let key = SigningKey::from_bytes(&[seed; 32]).unwrap();
    let address = UserAddress::from(key.verifying_key());
    bech32::encode::<Bech32m>(
        Hrp::parse("mn_addr_preprod").unwrap(),
        &hex::decode(serializable_hex(&address).unwrap()).unwrap(),
    )
    .unwrap()
}

#[test]
fn dapp_transfer_builds_multiple_outputs_atomically() {
    let token = "00".repeat(32);
    let state = state_with_unshielded(10, &token);
    let outputs = vec![
        DappTransactionOutput {
            wallet_type: "unshielded".to_owned(),
            token_type: token.clone(),
            amount: 3,
            receiver_address: target_address(8),
        },
        DappTransactionOutput {
            wallet_type: "unshielded".to_owned(),
            token_type: token,
            amount: 4,
            receiver_address: target_address(9),
        },
    ];
    let mut rng = StdRng::from_seed([7; 32]);
    let (proposed, raw) = state
        .build_dapp_transfer_with_rng(
            "preprod",
            &outputs,
            &secret("nightExternalKeyHex"),
            &secret("zswapSeedHex"),
            1_900_000_000,
            &mut rng,
        )
        .unwrap();
    let transaction: Transaction<
        crate::compat::Signature,
        ProofPreimageMarker,
        PedersenRandomness,
        InMemoryDB,
    > = tagged_deserialize(&raw[..]).unwrap();
    let Transaction::Standard(transaction) = transaction else {
        unreachable!("dApp transfer must be standard")
    };
    let intent = transaction.intents.get(&1).unwrap();
    let offer = intent.fallible_unshielded_offer.as_ref().unwrap();
    assert_eq!(offer.inputs.iter().count(), 1);
    assert_eq!(offer.outputs.iter().count(), 3);
    assert_eq!(state.unshielded.available_utxos.len(), 1);
    assert!(proposed.unshielded.available_utxos.is_empty());
    assert_eq!(proposed.unshielded.pending_utxos.len(), 1);
}

#[test]
fn dapp_intent_is_signed_without_reserving_open_offer_inputs() {
    let token = "02".repeat(32);
    let state = state_with_unshielded(10, &token);
    let inputs = vec![DappIntentInput {
        wallet_type: "unshielded".to_owned(),
        token_type: token.clone(),
        amount: 4,
    }];
    let outputs = vec![DappTransactionOutput {
        wallet_type: "unshielded".to_owned(),
        token_type: token,
        amount: 7,
        receiver_address: target_address(9),
    }];
    let mut rng = StdRng::from_seed([8; 32]);
    let raw = state
        .build_dapp_intent_with_rng(
            DappIntentBuildInput {
                network_id: "preprod",
                inputs: &inputs,
                outputs: &outputs,
                night_external_key: &secret("nightExternalKeyHex"),
                zswap_seed: &secret("zswapSeedHex"),
                ttl_seconds: 1_900_000_000,
            },
            &mut rng,
        )
        .unwrap();
    let transaction: Transaction<
        crate::compat::Signature,
        ProofPreimageMarker,
        PedersenRandomness,
        InMemoryDB,
    > = tagged_deserialize(&raw[..]).unwrap();
    let Transaction::Standard(transaction) = transaction else {
        unreachable!("dApp intent must be standard")
    };
    let intent = transaction.intents.get(&1).unwrap();
    let offer = intent.guaranteed_unshielded_offer.as_ref().unwrap();
    assert_eq!(offer.inputs.iter().count(), 1);
    assert_eq!(offer.outputs.iter().count(), 2);
    assert_eq!(offer.signatures.iter().count(), 1);
    assert_eq!(state.unshielded.available_utxos.len(), 1);
    assert!(state.unshielded.pending_utxos.is_empty());
}

#[test]
fn dapp_intent_rejects_zero_amount_outputs() {
    let token = "02".repeat(32);
    let state = state_with_unshielded(10, &token);
    let inputs = vec![DappIntentInput {
        wallet_type: "unshielded".to_owned(),
        token_type: token.clone(),
        amount: 4,
    }];
    let outputs = vec![DappTransactionOutput {
        wallet_type: "unshielded".to_owned(),
        token_type: token,
        amount: 0,
        receiver_address: target_address(9),
    }];
    let mut rng = StdRng::from_seed([8; 32]);
    assert!(matches!(
        state.build_dapp_intent_with_rng(
            DappIntentBuildInput {
                network_id: "preprod",
                inputs: &inputs,
                outputs: &outputs,
                night_external_key: &secret("nightExternalKeyHex"),
                zswap_seed: &secret("zswapSeedHex"),
                ttl_seconds: 1_900_000_000,
            },
            &mut rng,
        ),
        Err(MidnightRuntimeError::InvalidArgument)
    ));
}
use bech32::{Bech32m, Hrp};
