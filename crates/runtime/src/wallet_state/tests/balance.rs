use midnight_base_crypto::time::Timestamp;
use midnight_ledger::dust::{DustGenerationInfo, DustPublicKey, QualifiedDustOutput};
use midnight_ledger::structure::{
    INITIAL_PARAMETERS, ProofMarker, StandardTransaction, UnshieldedOffer, UtxoOutput, UtxoSpend,
};
use midnight_transient_crypto::commitment::PureGeneratorPedersen;
use midnight_zswap::Delta;
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::*;

const VIA_AMOUNT: u128 = 1_000_000;
const OTHER_TOKEN: &str = "0909090909090909090909090909090909090909090909090909090909090909";
const CURRENT_TIME: u64 = 10_000;
const TTL: u64 = 20_000;

type UnsealedIntent = Intent<Signature, ProofMarker, PedersenRandomness, InMemoryDB>;
type SealedTransaction = Transaction<Signature, ProofMarker, PureGeneratorPedersen, InMemoryDB>;

fn counterparty() -> UserAddress {
    UserAddress::from(SigningKey::from_bytes(&[9; 32]).unwrap().verifying_key())
}

fn token(hex_value: &str) -> UnshieldedTokenType {
    UnshieldedTokenType(decode_hash(hex_value).unwrap())
}

fn offer(
    inputs: Vec<UtxoSpend>,
    outputs: Vec<UtxoOutput>,
) -> UnshieldedOffer<Signature, InMemoryDB> {
    let mut inputs = inputs;
    let mut outputs = outputs;
    inputs.sort();
    outputs.sort();
    UnshieldedOffer {
        inputs: inputs.into_iter().collect(),
        outputs: outputs.into_iter().collect(),
        signatures: Vec::new().into_iter().collect(),
    }
}

/// An intent shaped like the one a dApp sends: it pays a counterparty out of an
/// unshielded offer it expects the connected wallet to fund.
fn dapp_intent(
    guaranteed: Option<UnshieldedOffer<Signature, InMemoryDB>>,
    fallible: Option<UnshieldedOffer<Signature, InMemoryDB>>,
) -> UnsealedIntent {
    Intent {
        guaranteed_unshielded_offer: guaranteed.map(Sp::new),
        fallible_unshielded_offer: fallible.map(Sp::new),
        actions: Vec::new().into_iter().collect(),
        dust_actions: None,
        ttl: Timestamp::from_secs(TTL),
        binding_commitment: PedersenRandomness::from(0),
    }
}

fn unsealed(intents: Vec<(u16, UnsealedIntent)>) -> Vec<u8> {
    let transaction: Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB> =
        Transaction::Standard(StandardTransaction {
            network_id: "preview".to_owned(),
            intents: intents.into_iter().collect(),
            guaranteed_coins: None,
            fallible_coins: LedgerHashMap::new(),
            binding_randomness: PedersenRandomness::from(0),
        });
    let mut raw = Vec::new();
    tagged_serialize(&transaction, &mut raw).unwrap();
    raw
}

fn sealed(intents: Vec<(u16, UnsealedIntent)>) -> Vec<u8> {
    let transaction: SealedTransaction = Transaction::Standard(StandardTransaction {
        network_id: "preview".to_owned(),
        intents: intents
            .into_iter()
            .map(|(segment, intent)| {
                (
                    segment,
                    Intent {
                        guaranteed_unshielded_offer: intent.guaranteed_unshielded_offer.clone(),
                        fallible_unshielded_offer: intent.fallible_unshielded_offer.clone(),
                        actions: Vec::new().into_iter().collect(),
                        dust_actions: None,
                        ttl: intent.ttl,
                        binding_commitment: PureGeneratorPedersen::largest_representable(),
                    },
                )
            })
            .collect(),
        guaranteed_coins: None,
        fallible_coins: LedgerHashMap::new(),
        binding_randomness: PedersenRandomness::from(0),
    });
    let mut raw = Vec::new();
    tagged_serialize(&transaction, &mut raw).unwrap();
    raw
}

/// The reproduced Via transaction: one unshielded payment of 1,000,000 NIGHT
/// that the wallet has to fund from its own UTXOs.
fn via_transaction() -> Vec<u8> {
    unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                Vec::new(),
                vec![UtxoOutput {
                    value: VIA_AMOUNT,
                    owner: counterparty(),
                    type_: token(NIGHT),
                }],
            )),
            None,
        ),
    )])
}

fn request<'a>(raw: &'a [u8], sealed: bool, fee_mode: FeeMode) -> BalanceRequest<'a> {
    BalanceRequest {
        network_id: "preview",
        original_raw: raw,
        sealed,
        ledger_parameters: &INITIAL_PARAMETERS,
        fee_blocks_margin: 0,
        additional_fee_overhead: 0,
        fee_mode,
        night_external_key: &[1; 32],
        zswap_seed: &[2; 32],
        dust_seed: &[3; 32],
        current_time_seconds: CURRENT_TIME,
        ttl_seconds: TTL,
    }
}

fn plan(state: &NativeWalletState, raw: &[u8], sealed: bool, fee_mode: FeeMode) -> BalancePlan {
    let mut rng = StdRng::seed_from_u64(7);
    state
        .plan_balance_with_rng(request(raw, sealed, fee_mode), &mut rng)
        .unwrap()
}

fn contribution(plan: &BalancePlan, wallet_type: &str, token_type: &str) -> Option<String> {
    plan.manifest
        .contributions
        .iter()
        .find(|entry| entry.wallet_type == wallet_type && entry.token_type == token_type)
        .map(|entry| entry.amount.clone())
}

fn change(plan: &BalancePlan, wallet_type: &str, token_type: &str) -> Option<String> {
    plan.manifest
        .change
        .iter()
        .find(|entry| entry.wallet_type == wallet_type && entry.token_type == token_type)
        .map(|entry| entry.amount.clone())
}

/// Re-derives the fully merged, proof-erased transaction the plan describes and
/// asserts the ledger sees no imbalance left in any token but DUST.
fn assert_token_balanced(raw: &[u8], sealed: bool, plan: &BalancePlan) {
    let base = plan.base_raw.clone().unwrap_or_else(|| raw.to_vec());
    let base_sealed = if plan.base_raw.is_some() {
        false
    } else {
        sealed
    };
    let mut merged =
        crate::transaction::decode_balance_original(&base, base_sealed, "preview").unwrap();
    if let Some(balancing) = &plan.balancing_raw {
        let balancing: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
            tagged_deserialize(&mut &balancing[..]).unwrap();
        merged = merged.merge(&balancing.erase_proofs()).unwrap();
    }
    let fees = merged.fees_with_margin(&INITIAL_PARAMETERS, 0).unwrap();
    for ((token_type, segment), value) in merged.balance(Some(fees)).unwrap() {
        if token_type == TokenType::Dust {
            continue;
        }
        assert_eq!(
            value, 0,
            "token {token_type:?} segment {segment} unbalanced"
        );
    }
}

/// Checks every signature in every unshielded offer of the rewritten base
/// transaction against the ledger's own well-formedness rule.
fn assert_signatures_valid(raw: &[u8]) {
    let transaction = crate::transaction::decode_unsealed_original(raw, "preview").unwrap();
    let Transaction::Standard(standard) = transaction else {
        unreachable!("fixtures are standard transactions")
    };
    for (segment, intent) in standard.intents.clone().into_iter() {
        let erased = intent.erase_proofs().erase_signatures();
        for offer in [
            intent.guaranteed_unshielded_offer.clone(),
            intent.fallible_unshielded_offer.clone(),
        ]
        .into_iter()
        .flatten()
        {
            (*offer)
                .clone()
                .well_formed(segment, &erased)
                .expect("offer must be well formed")()
            .expect("every input signature must verify");
        }
    }
}

#[test]
fn the_via_transaction_is_balanced_from_an_unshielded_wallet_input() {
    let (state, _keys, _address) = funded_state();
    let plan = plan(&state, &via_transaction(), false, FeeMode::Sponsored);

    assert_eq!(
        contribution(&plan, "unshielded", NIGHT).as_deref(),
        Some("1000000"),
        "the user must be shown the exact net debit, not the selected UTXO value"
    );
    assert_eq!(
        change(&plan, "unshielded", NIGHT).as_deref(),
        Some("999999000000")
    );
    let base = plan
        .base_raw
        .clone()
        .expect("an unsealed base is rewritten");
    assert_token_balanced(&via_transaction(), false, &plan);
    assert_signatures_valid(&base);
    assert!(
        plan.balancing_raw.is_none(),
        "an unshielded-only deficit needs no proven counterpart under sponsorship"
    );
}

#[test]
fn the_original_payment_survives_in_place_balancing() {
    let (state, _keys, _address) = funded_state();
    let raw = via_transaction();
    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    let base = plan.base_raw.clone().unwrap();
    let rewritten = crate::transaction::decode_unsealed_original(&base, "preview").unwrap();
    let Transaction::Standard(standard) = rewritten else {
        unreachable!()
    };
    let intent = standard.intents.get(&1).unwrap();
    let outputs = Vec::from(&intent.guaranteed_unshielded_offer.as_ref().unwrap().outputs);
    assert!(
        outputs.iter().any(|output| {
            output.value == VIA_AMOUNT
                && output.owner == counterparty()
                && output.type_ == token(NIGHT)
        }),
        "the dApp's own payment must still be present after balancing"
    );
    assert_eq!(standard.intents.iter().count(), 1, "no intent was added");
}

#[test]
fn balance_only_preparation_leaves_the_source_state_untouched() {
    let (state, _keys, _address) = funded_state();
    let before = state.unshielded.available_utxos.clone();
    let plan = plan(&state, &via_transaction(), false, FeeMode::Sponsored);
    assert_eq!(
        state.unshielded.available_utxos, before,
        "planning must never mutate the wallet state it reads"
    );
    assert!(
        plan.proposed
            .unshielded
            .pending_utxos
            .iter()
            .any(|coin| coin.utxo.output_no == 0),
        "the discardable clone is where a reservation would land"
    );
    assert!(
        state.unshielded.pending_utxos.is_empty(),
        "a preview must not reserve an input"
    );
}

#[test]
fn a_second_token_in_the_same_segment_is_balanced_too() {
    let (state, _keys, _address) = funded_state();
    let raw = unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                Vec::new(),
                vec![
                    UtxoOutput {
                        value: VIA_AMOUNT,
                        owner: counterparty(),
                        type_: token(NIGHT),
                    },
                    UtxoOutput {
                        value: 120,
                        owner: counterparty(),
                        type_: token(OTHER_TOKEN),
                    },
                ],
            )),
            None,
        ),
    )]);
    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    assert_eq!(
        contribution(&plan, "unshielded", NIGHT).as_deref(),
        Some("1000000")
    );
    assert_eq!(
        contribution(&plan, "unshielded", OTHER_TOKEN).as_deref(),
        Some("120")
    );
    assert_eq!(
        change(&plan, "unshielded", OTHER_TOKEN).as_deref(),
        Some("380")
    );
    assert_token_balanced(&raw, false, &plan);
    assert_signatures_valid(&plan.base_raw.clone().unwrap());
}

#[test]
fn a_fallible_deficit_is_balanced_inside_its_own_intent() {
    let (state, _keys, _address) = funded_state();
    let raw = unsealed(vec![(
        4,
        dapp_intent(
            None,
            Some(offer(
                Vec::new(),
                vec![UtxoOutput {
                    value: VIA_AMOUNT,
                    owner: counterparty(),
                    type_: token(NIGHT),
                }],
            )),
        ),
    )]);
    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    assert_eq!(
        contribution(&plan, "unshielded", NIGHT).as_deref(),
        Some("1000000")
    );
    let base = plan.base_raw.clone().unwrap();
    let rewritten = crate::transaction::decode_unsealed_original(&base, "preview").unwrap();
    let Transaction::Standard(standard) = rewritten else {
        unreachable!()
    };
    let intent = standard.intents.get(&4).unwrap();
    assert!(
        intent.guaranteed_unshielded_offer.is_none(),
        "a segment-4 deficit belongs in the fallible section"
    );
    assert_eq!(
        intent
            .fallible_unshielded_offer
            .as_ref()
            .unwrap()
            .inputs
            .len(),
        1
    );
    assert_token_balanced(&raw, false, &plan);
    assert_signatures_valid(&base);
}

#[test]
fn a_surplus_is_returned_to_the_wallet() {
    let (state, _keys, _address) = funded_state();
    // The dApp over-supplies the segment, so the wallet takes the excess back
    // instead of leaving the transaction unbalanced.
    let raw = unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                vec![UtxoSpend {
                    value: 900,
                    owner: SigningKey::from_bytes(&[9; 32]).unwrap().verifying_key(),
                    type_: token(OTHER_TOKEN),
                    intent_hash: IntentHash(decode_hash(&"aa".repeat(32)).unwrap()),
                    output_no: 0,
                }],
                Vec::new(),
            )),
            None,
        ),
    )]);
    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    assert_eq!(
        change(&plan, "unshielded", OTHER_TOKEN).as_deref(),
        Some("900")
    );
    assert_eq!(contribution(&plan, "unshielded", OTHER_TOKEN), None);
    assert_token_balanced(&raw, false, &plan);
}

#[test]
fn a_sealed_transaction_is_balanced_with_a_separate_intent() {
    let (state, _keys, _address) = funded_state();
    let raw = sealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                Vec::new(),
                vec![UtxoOutput {
                    value: VIA_AMOUNT,
                    owner: counterparty(),
                    type_: token(NIGHT),
                }],
            )),
            None,
        ),
    )]);
    let plan = plan(&state, &raw, true, FeeMode::Sponsored);
    assert!(
        plan.base_raw.is_none(),
        "a sealed transaction must be preserved byte for byte"
    );
    let balancing = plan
        .balancing_raw
        .clone()
        .expect("a sealed deficit needs a separate balancing transaction");
    let balancing: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&mut &balancing[..]).unwrap();
    let Transaction::Standard(standard) = &balancing else {
        unreachable!()
    };
    assert_eq!(standard.intents.iter().count(), 1);
    assert!(
        standard.intents.get(&1).is_none(),
        "the balancing intent must not collide with the sealed transaction's segment"
    );
    assert_eq!(
        contribution(&plan, "unshielded", NIGHT).as_deref(),
        Some("1000000")
    );
    assert_token_balanced(&raw, true, &plan);
}

#[test]
fn a_bound_fallible_deficit_is_reported_as_unsupported() {
    let (state, _keys, _address) = funded_state();
    let raw = sealed(vec![(
        4,
        dapp_intent(
            None,
            Some(offer(
                Vec::new(),
                vec![UtxoOutput {
                    value: VIA_AMOUNT,
                    owner: counterparty(),
                    type_: token(NIGHT),
                }],
            )),
        ),
    )]);
    let mut rng = StdRng::seed_from_u64(7);
    let error = state
        .plan_balance_with_rng(request(&raw, true, FeeMode::Sponsored), &mut rng)
        .unwrap_err();
    assert_eq!(error.to_string(), "UNSUPPORTED_TRANSACTION");
}

#[test]
fn a_shielded_deficit_is_balanced_with_a_proven_counteroffer() {
    let (state, _keys, _address) = funded_state();
    // A shielded segment's imbalance is read from the offer's deltas, so a
    // negative delta reproduces a dApp offer that creates shielded value it
    // expects the connected wallet to fund. The fixture is deliberately only
    // balance-shaped: the planner and the ledger's balance rule are what is
    // under test here, not the offer's own proofs.
    let transaction: Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB> =
        Transaction::Standard(StandardTransaction {
            network_id: "preview".to_owned(),
            intents: LedgerHashMap::new(),
            guaranteed_coins: Some(Sp::new(ZswapOffer {
                inputs: Vec::new().into_iter().collect(),
                outputs: Vec::new().into_iter().collect(),
                transient: Vec::new().into_iter().collect(),
                deltas: vec![Delta {
                    token_type: ShieldedTokenType(HashOutput([8; 32])),
                    value: -250,
                }]
                .into_iter()
                .collect(),
            })),
            fallible_coins: LedgerHashMap::new(),
            binding_randomness: PedersenRandomness::from(0),
        });
    let mut raw = Vec::new();
    tagged_serialize(&transaction, &mut raw).unwrap();

    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    assert_eq!(
        contribution(&plan, "shielded", &"08".repeat(32)).as_deref(),
        Some("250")
    );
    assert_eq!(
        change(&plan, "shielded", &"08".repeat(32)).as_deref(),
        Some("450"),
        "the 700 coin is split and the remainder returns to the wallet"
    );
    assert!(
        plan.base_raw.is_none(),
        "a shielded counteroffer never rewrites the caller's transaction"
    );
    assert!(
        plan.balancing_raw.is_some(),
        "shielded inputs need a proven counterpart"
    );
    assert_token_balanced(&raw, false, &plan);
}

#[test]
fn insufficient_token_funds_are_distinct_from_insufficient_dust() {
    let (state, _keys, _address) = funded_state();
    let raw = unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                Vec::new(),
                vec![UtxoOutput {
                    value: 5_000,
                    owner: counterparty(),
                    type_: token(OTHER_TOKEN),
                }],
            )),
            None,
        ),
    )]);
    let mut rng = StdRng::seed_from_u64(7);
    let error = state
        .plan_balance_with_rng(request(&raw, false, FeeMode::Sponsored), &mut rng)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "INSUFFICIENT_FUNDS",
        "a token shortfall must never be reported as a DUST shortfall"
    );

    // The same wallet holds the NIGHT the Via transaction needs but no DUST, so
    // the local-fee path fails on DUST alone.
    let dust_error = state
        .plan_balance_with_rng(
            request(&via_transaction(), false, FeeMode::LocalDust),
            &mut rng,
        )
        .unwrap_err();
    assert_eq!(dust_error.to_string(), "INSUFFICIENT_DUST");
}

#[test]
fn sponsored_fee_mode_reports_no_local_dust() {
    let (state, _keys, _address) = funded_state();
    let plan = plan(&state, &via_transaction(), false, FeeMode::Sponsored);
    assert_eq!(plan.manifest.dust, "sponsored");
}

#[test]
fn an_approved_manifest_only_authorizes_an_equal_or_cheaper_plan() {
    let (state, _keys, _address) = funded_state();
    let plan = plan(&state, &via_transaction(), false, FeeMode::Sponsored);
    let approved = plan.manifest.clone();
    assert!(approved.authorizes(&approved).is_ok());

    let mut cheaper = approved.clone();
    cheaper.dust = "5".to_owned();
    let mut costlier = approved.clone();
    costlier.dust = "500".to_owned();
    let mut local = approved.clone();
    local.dust = "100".to_owned();
    assert!(
        local.authorizes(&cheaper).is_ok(),
        "a cheaper fee is allowed"
    );
    assert_eq!(
        local.authorizes(&costlier).unwrap_err().to_string(),
        "BALANCE_APPROVAL_CHANGED"
    );
    assert_eq!(
        approved.authorizes(&local).unwrap_err().to_string(),
        "BALANCE_APPROVAL_CHANGED",
        "sponsorship must not silently become a local DUST spend"
    );

    let mut moved = approved.clone();
    moved.contributions[0].amount = "2000000".to_owned();
    assert_eq!(
        approved.authorizes(&moved).unwrap_err().to_string(),
        "BALANCE_APPROVAL_CHANGED"
    );

    let mut other_transaction = approved.clone();
    other_transaction.transaction_digest = "ff".repeat(32);
    assert_eq!(
        approved
            .authorizes(&other_transaction)
            .unwrap_err()
            .to_string(),
        "BALANCE_APPROVAL_CHANGED"
    );

    let mut stale = approved.clone();
    stale.wallet_state_digest = "ee".repeat(32);
    assert_eq!(
        approved.authorizes(&stale).unwrap_err().to_string(),
        "BALANCE_APPROVAL_CHANGED",
        "a wallet whose coin set moved must be re-approved"
    );
}

#[test]
fn the_manifest_digest_changes_with_every_field() {
    let (state, _keys, _address) = funded_state();
    let manifest = plan(&state, &via_transaction(), false, FeeMode::Sponsored).manifest;
    let baseline = manifest.digest();
    assert_eq!(baseline.len(), 64);
    assert_eq!(manifest.digest(), baseline, "the digest is deterministic");

    type Mutation = Box<dyn Fn(&mut BalanceManifest)>;
    let mutate: Vec<Mutation> = vec![
        Box::new(|value| value.transaction_digest = "00".repeat(32)),
        Box::new(|value| value.variant = "sealed".to_owned()),
        Box::new(|value| value.dust = "7".to_owned()),
        Box::new(|value| value.wallet_state_digest = "00".repeat(32)),
        Box::new(|value| value.contributions.clear()),
        Box::new(|value| value.change.clear()),
    ];
    for apply in mutate {
        let mut changed = manifest.clone();
        apply(&mut changed);
        assert_ne!(changed.digest(), baseline);
    }
}

#[test]
fn a_wallet_with_no_imbalance_to_cover_needs_no_counterpart() {
    let (state, _keys, _address) = funded_state();
    let raw = unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                vec![UtxoSpend {
                    value: 700,
                    owner: SigningKey::from_bytes(&[9; 32]).unwrap().verifying_key(),
                    type_: token(OTHER_TOKEN),
                    intent_hash: IntentHash(decode_hash(&"bb".repeat(32)).unwrap()),
                    output_no: 3,
                }],
                vec![UtxoOutput {
                    value: 700,
                    owner: counterparty(),
                    type_: token(OTHER_TOKEN),
                }],
            )),
            None,
        ),
    )]);
    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    assert!(plan.base_raw.is_none() && plan.balancing_raw.is_none());
    assert!(plan.manifest.contributions.is_empty());
    assert!(plan.manifest.change.is_empty());
}

/// A wallet holding one fully generated DUST output, built directly from the
/// ledger's local-state primitives. Replaying real DUST events would need a
/// node; what these tests need is a spendable output with a known value.
fn dust_funded_state() -> NativeWalletState {
    let (mut state, _keys, _address) = funded_state();
    let secret = DustSecretKey::derive_secret_key(&[3; 32]);
    let owner = DustPublicKey::from(secret.clone());
    let backing_night =
        midnight_ledger::dust::initial_nonce(0, IntentHash(decode_hash(&"cc".repeat(32)).unwrap()));
    let generation = DustGenerationInfo {
        // One NIGHT backs the output, which caps it at five DUST.
        value: 1_000_000,
        owner,
        nonce: backing_night,
        // Decay starts far beyond the window these tests run in.
        dtime: Timestamp::from_secs(1_000_000_000),
    };
    let utxo = QualifiedDustOutput {
        initial_value: 5_000_000_000_000_000,
        owner,
        nonce: midnight_ledger::dust::dust_first_nonce(&backing_night, &owner),
        seq: 0,
        ctime: Timestamp::from_secs(0),
        backing_night,
        mt_index: 0,
    };
    state.dust = state
        .dust
        .insert_generation_info(0, generation, Some(backing_night))
        .unwrap()
        .insert_commitment(0, utxo, true)
        .unwrap()
        .add_utxo(&utxo.nullifier(&secret), &utxo, None)
        .unwrap();
    state
}

#[test]
fn local_dust_pays_the_complete_fee_of_the_balanced_transaction() {
    let state = dust_funded_state();
    let raw = via_transaction();
    let local = plan(&state, &raw, false, FeeMode::LocalDust);

    let spent = local
        .manifest
        .dust
        .parse::<u128>()
        .expect("local mode reports a decimal DUST ceiling");
    assert!(spent > 0, "a real transaction always costs some DUST");

    // The token legs and the DUST leg must both come out at zero once the
    // wallet's own counterpart is merged in.
    let base = local.base_raw.clone().unwrap();
    let mut merged = crate::transaction::decode_balance_original(&base, false, "preview").unwrap();
    let balancing: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&mut &local.balancing_raw.clone().unwrap()[..]).unwrap();
    merged = merged.merge(&balancing.erase_proofs()).unwrap();
    let fees = merged.fees_with_margin(&INITIAL_PARAMETERS, 0).unwrap();
    for ((token_type, segment), value) in merged.balance(Some(fees)).unwrap() {
        assert_eq!(
            value, 0,
            "token {token_type:?} segment {segment} left unbalanced"
        );
    }
    assert_eq!(fees, spent, "the manifest must quote the fee actually paid");
    assert_signatures_valid(&base);

    // The same wallet under sponsorship spends no DUST at all.
    let sponsored = plan(&state, &raw, false, FeeMode::Sponsored);
    assert_eq!(sponsored.manifest.dust, "sponsored");
    assert_eq!(
        sponsored.manifest.contributions, local.manifest.contributions,
        "fee routing must not change what the user pays in tokens"
    );
}
