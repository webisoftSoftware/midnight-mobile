use super::*;

/// A wallet holding one fully generated DUST output, built directly from the
/// ledger's local-state primitives. Replaying real DUST events would need a
/// node; what these tests need is a spendable output with a known value.
pub(super) fn dust_funded_state() -> NativeWalletState {
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
    let Transaction::Standard(standard) = &balancing else {
        unreachable!()
    };
    let ctime = match standard.intents.get(&2) {
        Some(intent) => intent.dust_actions.as_ref().map(|actions| actions.ctime),
        None => None,
    };
    assert_eq!(
        ctime,
        Some(Timestamp::from_secs(CURRENT_TIME - 30)),
        "DUST spend time must be before the wall-clock time"
    );
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
