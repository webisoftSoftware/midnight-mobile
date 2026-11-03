use super::*;

/// Golden parity with the pinned extension Wallet SDK balancer.
///
/// The fixture is shared: the extension's
/// `tests/unit/domain/balance-golden-parity.test.ts` drives the SDK's own
/// balancer over the same scenarios. Mobile balances in Rust and never loads
/// the ledger WASM, so this file is the only place the two implementations
/// can be held to one answer about what a user pays.
const GOLDEN_BALANCE: &str = include_str!("../../../../fixtures/balance-golden.json");

/// Builds a wallet holding exactly the scenario's coins, and a transaction
/// whose guaranteed offer reproduces exactly the scenario's imbalances: an
/// output for a deficit the wallet must fund, a counterparty input for a
/// surplus the wallet takes back.
fn golden_case(scenario: &serde_json::Value) -> (NativeWalletState, Vec<u8>) {
    let material = material();
    let mut state = empty_state();
    for coin in scenario["coins"].as_array().unwrap() {
        state
            .unshielded
            .available_utxos
            .push(UnshieldedUtxoWithMeta {
                utxo: UnshieldedUtxo {
                    value: coin["value"].as_str().unwrap().to_owned(),
                    owner: material.unshielded_address_hex.clone(),
                    type_: coin["type"].as_str().unwrap().to_owned(),
                    intent_hash: coin["intentHash"].as_str().unwrap().to_owned(),
                    output_no: coin["outputNo"].as_u64().unwrap() as u32,
                },
                meta: UnshieldedMeta {
                    ctime: 1,
                    registered_for_dust_generation: false,
                },
            });
    }

    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for (token_hex, raw_amount) in scenario["imbalances"].as_object().unwrap() {
        let token = token(token_hex);
        let amount = raw_amount.as_str().unwrap().parse::<i128>().unwrap();
        if amount < 0 {
            outputs.push(UtxoOutput {
                value: amount.unsigned_abs(),
                owner: counterparty(),
                type_: token,
            });
            continue;
        }
        // A surplus means the transaction already declares spending one of
        // the wallet's own UTXOs without paying it out, so the input has to
        // be that exact coin: the wallet signs it and takes the value back.
        let coin = scenario["coins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|coin| {
                coin["type"].as_str() == Some(&token_hex[..])
                    && coin["value"].as_str().unwrap().parse::<i128>() == Ok(amount)
            })
            .expect("a surplus scenario must declare the coin it over-collects");
        inputs.push(UtxoSpend {
            value: amount.unsigned_abs(),
            owner: crate::compat::owner_key(&SigningKey::from_bytes(&[1; 32]).unwrap()),
            type_: token,
            intent_hash: IntentHash(decode_hash(coin["intentHash"].as_str().unwrap()).unwrap()),
            output_no: coin["outputNo"].as_u64().unwrap() as u32,
        });
    }
    let raw = unsealed(vec![(1, dapp_intent(Some(offer(inputs, outputs)), None))]);
    (state, raw)
}

fn golden_entries(
    entries: &[crate::wallet_state::balance::WalletContribution],
) -> Vec<(String, String)> {
    let mut values = entries
        .iter()
        .map(|entry| (entry.token_type.clone(), entry.amount.clone()))
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn golden_expected(scenario: &serde_json::Value, field: &str) -> Vec<(String, String)> {
    let mut values = scenario[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["tokenType"].as_str().unwrap().to_owned(),
                entry["amount"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    values.sort();
    values
}

#[test]
fn the_planner_matches_the_wallet_sdk_on_every_golden_scenario() {
    let golden: serde_json::Value = serde_json::from_str(GOLDEN_BALANCE).unwrap();
    let scenarios = golden["scenarios"].as_array().unwrap();
    assert!(
        scenarios
            .iter()
            .any(|scenario| scenario["name"] == "via-unshielded-night-payment"),
        "the reproduced Via transaction must stay in the shared fixture"
    );
    for scenario in scenarios {
        let name = scenario["name"].as_str().unwrap();
        let (state, raw) = golden_case(scenario);
        // Sponsored mode keeps the comparison to the token legs, which is
        // all the SDK's unshielded balancer decides.
        let plan = plan(&state, &raw, false, FeeMode::Sponsored);
        assert_eq!(
            golden_entries(&plan.manifest.contributions),
            golden_expected(scenario, "contributions"),
            "scenario {name} disagreed on the net contribution"
        );
        assert_eq!(
            golden_entries(&plan.manifest.change),
            golden_expected(scenario, "change"),
            "scenario {name} disagreed on the change"
        );
        assert_token_balanced(&raw, false, &plan);
        assert_signatures_valid(&plan.base_raw.clone().unwrap());
    }
}
