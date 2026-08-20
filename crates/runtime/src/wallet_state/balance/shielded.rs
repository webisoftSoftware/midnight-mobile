use super::*;

/// One segment's shielded counteroffer, together with the approval figures.
#[derive(Default)]
pub(super) struct ShieldedPlan {
    pub(super) offer: Option<ZswapOffer<ProofPreimage, InMemoryDB>>,
    pub(super) contributions: BTreeMap<ShieldedTokenType, u128>,
    pub(super) change: BTreeMap<ShieldedTokenType, u128>,
}

/// Resolves the shielded imbalances of one segment against the wallet.
///
/// Shielded inputs always need a proof, so the counteroffer is returned for a
/// separate balancing transaction rather than spliced into the original: that
/// keeps the caller's transaction content untouched and works identically for
/// sealed and unsealed originals.
pub(super) fn plan_segment<R: Rng + CryptoRng + ?Sized>(
    proposed: &mut NativeWalletState,
    secret_keys: &ZswapSecretKeys,
    segment: u16,
    imbalances: &BTreeMap<ShieldedTokenType, i128>,
    rng: &mut R,
) -> Result<ShieldedPlan, MidnightRuntimeError> {
    let mut plan = ShieldedPlan::default();
    if imbalances.is_empty() {
        return Ok(plan);
    }
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for (token, amount) in imbalances {
        if *amount > 0 {
            let surplus =
                u128::try_from(*amount).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            outputs.push(watched_output(
                proposed,
                secret_keys,
                segment,
                *token,
                surplus,
                rng,
            )?);
            let entry = plan.change.entry(*token).or_default();
            *entry = entry.saturating_add(surplus);
            continue;
        }
        let needed = amount
            .checked_neg()
            .and_then(|value| u128::try_from(value).ok())
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        // Smallest coin first, tie-broken by nullifier, so the selection is
        // reproducible across the preview and the execution that follows it.
        let mut available = proposed
            .shielded
            .coins
            .iter()
            .filter(|(nullifier, coin)| {
                coin.type_ == *token && !proposed.shielded.pending_spends.contains_key(nullifier)
            })
            .map(|(nullifier, coin)| Ok((*coin, serializable_hex(&nullifier)?)))
            .collect::<Result<Vec<_>, MidnightRuntimeError>>()?;
        available.sort_by(|left, right| {
            left.0
                .value
                .cmp(&right.0.value)
                .then_with(|| left.1.cmp(&right.1))
        });

        let mut selected = 0_u128;
        for (coin, _) in available {
            let (next, input) = proposed
                .shielded
                .spend(rng, secret_keys, &coin, Some(segment))
                .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            proposed.shielded = next;
            inputs.push(input);
            selected = selected
                .checked_add(coin.value)
                .ok_or(MidnightRuntimeError::InvalidArgument)?;
            if selected >= needed {
                break;
            }
        }
        if selected < needed {
            return Err(MidnightRuntimeError::InsufficientFunds);
        }
        let change = selected - needed;
        if change > 0 {
            outputs.push(watched_output(
                proposed,
                secret_keys,
                segment,
                *token,
                change,
                rng,
            )?);
            let entry = plan.change.entry(*token).or_default();
            *entry = entry.saturating_add(change);
        }
        let entry = plan.contributions.entry(*token).or_default();
        *entry = entry.saturating_add(needed);
    }
    if inputs.is_empty() && outputs.is_empty() {
        return Ok(plan);
    }
    proposed.refresh_coin_hashes(secret_keys)?;
    plan.offer = Some(
        ZswapOffer::new(inputs, outputs, Vec::new())
            .ok_or(MidnightRuntimeError::InvalidArgument)?,
    );
    Ok(plan)
}

/// Creates an output back to this wallet and starts watching for it, so the
/// change is recoverable from the wallet's own state after submission.
fn watched_output<R: Rng + CryptoRng + ?Sized>(
    proposed: &mut NativeWalletState,
    secret_keys: &ZswapSecretKeys,
    segment: u16,
    token: ShieldedTokenType,
    value: u128,
    rng: &mut R,
) -> Result<ZswapOutput<ProofPreimage, InMemoryDB>, MidnightRuntimeError> {
    let coin = ShieldedCoinInfo::new(rng, value, token);
    let output = ZswapOutput::<ProofPreimage, InMemoryDB>::new(
        rng,
        &coin,
        Some(segment),
        &secret_keys.coin_public_key(),
        Some(secret_keys.enc_public_key()),
    )
    .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    proposed.shielded = proposed
        .shielded
        .watch_for(&secret_keys.coin_public_key(), &coin);
    Ok(output)
}
