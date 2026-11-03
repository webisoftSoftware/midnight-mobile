use super::*;

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
