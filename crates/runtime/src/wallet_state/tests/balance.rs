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
mod dust;
mod golden;
mod manifest;
use dust::dust_funded_state;

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

fn signed_intent(intent: UnsealedIntent, segment: u16, signing_key: &SigningKey) -> UnsealedIntent {
    let mut rng = StdRng::seed_from_u64(19);
    let data_to_sign = intent
        .erase_proofs()
        .erase_signatures()
        .data_to_sign(segment);
    let signature = crate::compat::sign(signing_key, &mut rng, &data_to_sign);
    let mut signed = intent;
    for offer in [
        signed.guaranteed_unshielded_offer.as_mut(),
        signed.fallible_unshielded_offer.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        let mut updated_offer = (**offer).clone();
        let count = updated_offer.inputs.len();
        updated_offer.add_signatures(vec![signature.clone(); count]);
        *offer = Sp::new(updated_offer);
    }
    signed
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
        materialize: true,
    }
}

fn preview_request<'a>(raw: &'a [u8], sealed: bool, fee_mode: FeeMode) -> BalanceRequest<'a> {
    let mut request = request(raw, sealed, fee_mode);
    request.materialize = false;
    request
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
fn preview_planning_does_not_materialize_unshielded_signatures() {
    let (state, _keys, _address) = funded_state();
    let raw = via_transaction();
    let mut rng = StdRng::seed_from_u64(7);
    let plan = state
        .plan_balance_with_rng(preview_request(&raw, false, FeeMode::Sponsored), &mut rng)
        .unwrap();
    let base = plan
        .base_raw
        .expect("preview must include the changed transaction");
    let Transaction::Standard(standard) =
        crate::transaction::decode_unsealed_original(&base, "preview").unwrap()
    else {
        unreachable!()
    };
    let intent = standard.intents.get(&1).unwrap();
    for offer in [
        intent.guaranteed_unshielded_offer.as_ref(),
        intent.fallible_unshielded_offer.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        assert_eq!(
            Vec::from(&offer.signatures),
            vec![Signature::default(); offer.inputs.len()]
        );
    }
}

#[test]
fn preview_and_execution_quote_the_same_dust_for_unshielded_funding() {
    let state = dust_funded_state();
    let raw = via_transaction();
    let mut preview_rng = StdRng::seed_from_u64(7);
    let preview = state
        .plan_balance_with_rng(
            preview_request(&raw, false, FeeMode::LocalDust),
            &mut preview_rng,
        )
        .unwrap();
    let mut execution_rng = StdRng::seed_from_u64(7);
    let execution = state
        .plan_balance_with_rng(request(&raw, false, FeeMode::LocalDust), &mut execution_rng)
        .unwrap();
    assert_eq!(preview.manifest.dust, execution.manifest.dust);
    assert_eq!(
        preview.manifest.contributions,
        execution.manifest.contributions
    );
    assert_eq!(preview.manifest.change, execution.manifest.change);
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
    // The dApp declares spending one of the wallet's own UTXOs without paying
    // it out, so the wallet signs that input and takes the surplus back rather
    // than leaving the transaction unbalanced.
    let raw = unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                vec![UtxoSpend {
                    value: 500,
                    owner: crate::compat::owner_key(&SigningKey::from_bytes(&[1; 32]).unwrap()),
                    type_: token(OTHER_TOKEN),
                    intent_hash: IntentHash(decode_hash(&format!("{:064x}", 2)).unwrap()),
                    output_no: 1,
                }],
                Vec::new(),
            )),
            None,
        ),
    )]);
    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    assert_eq!(
        change(&plan, "unshielded", OTHER_TOKEN).as_deref(),
        Some("500")
    );
    assert_eq!(contribution(&plan, "unshielded", OTHER_TOKEN), None);
    assert_token_balanced(&raw, false, &plan);
    assert_signatures_valid(&plan.base_raw.clone().unwrap());
}

#[test]
fn an_offer_carrying_an_unsigned_counterparty_input_is_unsupported() {
    let (state, _keys, _address) = funded_state();
    // Adding the wallet's inputs changes the intent's signature data, so a
    // counterparty's input can only survive if it is already signed. Signing it
    // here would be a forgery, and leaving a gap would misattribute the next
    // signature, so the request is refused instead.
    let raw = unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                vec![UtxoSpend {
                    value: 900,
                    owner: crate::compat::owner_key(&SigningKey::from_bytes(&[9; 32]).unwrap()),
                    type_: token(OTHER_TOKEN),
                    intent_hash: IntentHash(decode_hash(&"aa".repeat(32)).unwrap()),
                    output_no: 0,
                }],
                Vec::new(),
            )),
            None,
        ),
    )]);
    let mut rng = StdRng::seed_from_u64(7);
    let error = state
        .plan_balance_with_rng(request(&raw, false, FeeMode::Sponsored), &mut rng)
        .unwrap_err();
    assert_eq!(error.to_string(), "UNSUPPORTED_TRANSACTION");
}

#[test]
fn an_offer_carrying_a_signed_counterparty_input_is_unsupported() {
    let (state, _keys, _address) = funded_state();
    let foreign = SigningKey::from_bytes(&[9; 32]).unwrap();
    let raw = unsealed(vec![(
        1,
        signed_intent(
            dapp_intent(
                Some(offer(
                    vec![UtxoSpend {
                        value: 900,
                        owner: crate::compat::owner_key(&foreign),
                        type_: token(OTHER_TOKEN),
                        intent_hash: IntentHash(decode_hash(&"aa".repeat(32)).unwrap()),
                        output_no: 0,
                    }],
                    Vec::new(),
                )),
                None,
            ),
            1,
            &foreign,
        ),
    )]);
    let mut rng = StdRng::seed_from_u64(7);
    let error = state
        .plan_balance_with_rng(request(&raw, false, FeeMode::Sponsored), &mut rng)
        .unwrap_err();
    assert_eq!(error.to_string(), "UNSUPPORTED_TRANSACTION");
}

#[test]
fn wallet_owned_signatures_are_refreshed_across_both_offer_sections() {
    let (state, _keys, _address) = funded_state();
    let wallet = SigningKey::from_bytes(&[1; 32]).unwrap();
    let raw = unsealed(vec![(
        1,
        signed_intent(
            dapp_intent(
                Some(offer(
                    Vec::new(),
                    vec![UtxoOutput {
                        value: VIA_AMOUNT,
                        owner: counterparty(),
                        type_: token(NIGHT),
                    }],
                )),
                Some(offer(
                    vec![UtxoSpend {
                        value: 500,
                        owner: crate::compat::owner_key(&wallet),
                        type_: token(OTHER_TOKEN),
                        intent_hash: IntentHash(decode_hash(&"bb".repeat(32)).unwrap()),
                        output_no: 0,
                    }],
                    Vec::new(),
                )),
            ),
            1,
            &wallet,
        ),
    )]);
    let plan = plan(&state, &raw, false, FeeMode::Sponsored);
    assert_signatures_valid(&plan.base_raw.unwrap());
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
fn a_wallet_with_no_imbalance_to_cover_needs_no_counterpart() {
    let (state, _keys, _address) = funded_state();
    let raw = unsealed(vec![(
        1,
        dapp_intent(
            Some(offer(
                vec![UtxoSpend {
                    value: 700,
                    owner: crate::compat::owner_key(&SigningKey::from_bytes(&[9; 32]).unwrap()),
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
