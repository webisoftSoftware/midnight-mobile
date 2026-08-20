use super::*;

pub(super) struct DustRequest<'a> {
    pub(super) network_id: &'a str,
    /// The proof-erased transaction the fee is calculated for: the caller's
    /// transaction already merged with the wallet's token counteroffers.
    pub(super) base: &'a Transaction<Signature, (), Pedersen, InMemoryDB>,
    pub(super) ledger_parameters: &'a LedgerParameters,
    pub(super) fee_blocks_margin: usize,
    pub(super) additional_fee_overhead: u128,
    pub(super) dust_seed: &'a [u8],
    pub(super) current_time_seconds: u64,
    pub(super) ttl_seconds: u64,
    pub(super) segment: u16,
}

/// One convergence candidate: the state after the spends, the intent that
/// carries them, and the standalone transaction used to price it.
type Candidate = (
    DustLocalState<InMemoryDB>,
    UnprovenIntent,
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>,
);

pub(super) struct DustBalance {
    pub(super) state: DustLocalState<InMemoryDB>,
    pub(super) intent: Option<UnprovenIntent>,
    pub(super) spent: u128,
}

/// Builds the DUST spends that pay the complete fee of `base`.
///
/// Every non-DUST imbalance must already be resolved: fee sponsorship must
/// never be able to hide a token deficit that belongs to the wallet that owns
/// that token. The spend values are part of the serialized size and therefore
/// of the fee, so the loop re-derives the fee from the exact candidate until it
/// converges.
pub(super) fn plan(
    dust_state: &DustLocalState<InMemoryDB>,
    request: DustRequest<'_>,
) -> Result<DustBalance, MidnightRuntimeError> {
    let DustRequest {
        network_id,
        base,
        ledger_parameters,
        fee_blocks_margin,
        additional_fee_overhead,
        dust_seed,
        current_time_seconds,
        ttl_seconds,
        segment,
    } = request;
    if network_id.is_empty()
        || ttl_seconds <= current_time_seconds
        || dust_state.params != ledger_parameters.dust
    {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    let mut seed: [u8; 32] = dust_seed
        .try_into()
        .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
    let dust_key = DustSecretKey::derive_secret_key(&seed);
    seed.zeroize();
    let current_time = midnight_base_crypto::time::Timestamp::from_secs(current_time_seconds);
    let ttl = midnight_base_crypto::time::Timestamp::from_secs(ttl_seconds);

    let mut available = dust_state
        .utxos()
        .filter_map(|coin| {
            let generation = dust_state.generation_info(&coin)?;
            let value = midnight_ledger::dust::DustOutput::from(coin).updated_value(
                &generation,
                current_time,
                &dust_state.params,
            );
            (value > 0).then_some((value, coin))
        })
        .collect::<Vec<_>>();
    available.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.nonce.as_le_bytes().cmp(&right.1.nonce.as_le_bytes()))
    });

    let fee_shortfall = |total_fee: u128| -> Result<u128, MidnightRuntimeError> {
        let balances = base
            .balance(Some(total_fee))
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        for ((token, candidate_segment), value) in &balances {
            if (*token, *candidate_segment) != (TokenType::Dust, 0) && *value != 0 {
                // Token balancing runs before this stage, so a surviving
                // non-DUST imbalance is a planner defect, not user input.
                return Err(MidnightRuntimeError::NativeInternal);
            }
        }
        match balances.get(&(TokenType::Dust, 0)).copied().unwrap_or(0) {
            value if value < 0 => value
                .checked_neg()
                .and_then(|value| u128::try_from(value).ok())
                .ok_or(MidnightRuntimeError::InvalidArgument),
            0 => Ok(0),
            _ => Err(MidnightRuntimeError::InvalidArgument),
        }
    };

    let build = |deductions: &[u128]| -> Result<Candidate, MidnightRuntimeError> {
        let mut next_state = dust_state.clone();
        let mut spends = Vec::with_capacity(deductions.len());
        for ((_, coin), deduction) in available.iter().zip(deductions) {
            let (next, spend) = next_state
                .spend(&dust_key, coin, *deduction, current_time)
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            next_state = next;
            spends.push(spend);
        }
        let actions = DustActions::<Signature, ProofPreimageMarker, InMemoryDB> {
            spends: spends.into_iter().collect(),
            registrations: Vec::new().into_iter().collect(),
            ctime: current_time,
        };
        let intent = Intent::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
            &mut OsRng,
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(actions),
            ttl,
        );
        let transaction = Transaction::new(
            network_id.to_owned(),
            [(segment, intent.clone())].into_iter().collect(),
            None,
            LedgerHashMap::new(),
        );
        Ok((next_state, intent, transaction))
    };

    let combined_fee = |candidate: &Transaction<
        Signature,
        ProofPreimageMarker,
        PedersenRandomness,
        InMemoryDB,
    >|
     -> Result<u128, MidnightRuntimeError> {
        base.merge(&candidate.erase_proofs())
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?
            .fees_with_margin(ledger_parameters, fee_blocks_margin)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?
            .checked_add(additional_fee_overhead)
            .ok_or(MidnightRuntimeError::InvalidArgument)
    };

    let base_fee = base
        .fees_with_margin(ledger_parameters, fee_blocks_margin)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?
        .checked_add(additional_fee_overhead)
        .ok_or(MidnightRuntimeError::InvalidArgument)?;
    let mut target = fee_shortfall(base_fee)?;
    if target == 0 {
        return Ok(DustBalance {
            state: dust_state.clone(),
            intent: None,
            spent: 0,
        });
    }

    let mut selected_count = 0_usize;
    let mut deductions = Vec::<u128>::new();
    for _ in 0..64 {
        while available
            .iter()
            .take(selected_count)
            .map(|(value, _)| *value)
            .fold(0_u128, u128::saturating_add)
            < target
        {
            if selected_count >= available.len() {
                return Err(MidnightRuntimeError::InsufficientDust);
            }
            selected_count += 1;
        }

        deductions.clear();
        let mut remaining = target;
        for (value, _) in available.iter().take(selected_count) {
            let deduction = remaining.min(*value);
            deductions.push(deduction);
            remaining -= deduction;
        }
        if remaining != 0 {
            return Err(MidnightRuntimeError::InsufficientDust);
        }

        let (candidate_state, candidate_intent, candidate_transaction) = build(&deductions)?;
        let exact_target = fee_shortfall(combined_fee(&candidate_transaction)?)?;
        if exact_target == target {
            return Ok(DustBalance {
                state: candidate_state,
                intent: Some(candidate_intent),
                spent: target,
            });
        }
        target = exact_target;
    }
    Err(MidnightRuntimeError::InsufficientDust)
}
