{
if let RuntimeCommand::Transfer {
    to,
    amount,
    token_type,
    wallet_type,
} = &command
{
    if !["shielded", "unshielded", "dust"]
        .iter()
        .all(|stream| state.caught_up_streams.contains(*stream))
    {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let amount = amount
        .parse::<u128>()
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let (proposed, transaction) = match wallet_type.as_str() {
        "shielded" => state.wallet_state.build_shielded_transfer(
            &state.config.network_id,
            to,
            amount,
            token_type,
            &state.secrets.zswap_seed,
        )?,
        "unshielded" => {
            let ttl_seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| MidnightRuntimeError::NativeInternal)?
                .as_secs()
                .checked_add(30 * 60)
                .ok_or(MidnightRuntimeError::NativeInternal)?;
            state.wallet_state.build_unshielded_transfer(
                &state.config.network_id,
                to,
                amount,
                token_type,
                &state.secrets.night_external_key,
                ttl_seconds,
            )?
        }
        _ => return Err(MidnightRuntimeError::InvalidArgument),
    };
    let expected_identifiers =
        transaction::validate_unproven_transaction(&transaction, &state.config.network_id)?;
    drop(state);
    let (operation, effect_id) = register_operation(
        session_id,
        generation,
        &session,
        PendingOperationKind::Transfer {
            proposed_state: proposed,
            expected_identifiers,
        },
    )?;
    return to_json(&OperationStep {
        kind: "network",
        operation: Some(operation),
        effect_id: Some(effect_id),
        effect: Some("proveAndBalance"),
        endpoint_role: Some("proof"),
        body_base64: Some(encode_base64(&transaction)),
        result_json: None,
    });
}

if let RuntimeCommand::GenerateDust {
    ledger_parameters_base64,
    fee_blocks_margin,
    additional_fee_overhead,
} = &command
{
    if !["shielded", "unshielded", "dust"]
        .iter()
        .all(|stream| state.caught_up_streams.contains(*stream))
    {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let current_time_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?
        .as_secs();
    let ttl_seconds = current_time_seconds
        .checked_add(30 * 60)
        .ok_or(MidnightRuntimeError::NativeInternal)?;
    let parameters_raw = decode_base64(ledger_parameters_base64)?;
    let ledger_parameters = transaction::decode_ledger_parameters(&parameters_raw)?;
    let fee_blocks_margin = usize::try_from(*fee_blocks_margin)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if fee_blocks_margin > 64 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let additional_fee_overhead_value = additional_fee_overhead
        .parse::<u128>()
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if additional_fee_overhead_value.to_string() != *additional_fee_overhead {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let (proposed, transaction) = state.wallet_state.build_dust_registration(
        &state.config.network_id,
        &state.secrets.night_external_key,
        &state.secrets.dust_seed,
        current_time_seconds,
        ttl_seconds,
    )?;
    state.wallet_state.validate_dust_registration_fee(
        &transaction,
        &ledger_parameters,
        fee_blocks_margin,
        additional_fee_overhead_value,
    )?;
    let expected_identifiers =
        transaction::validate_unproven_transaction(&transaction, &state.config.network_id)?;
    let responses = transaction::RemoteProofResponses::default();
    match transaction::advance_unproven_transaction(
        &transaction,
        &state.config.network_id,
        &responses,
    )? {
        transaction::BalanceProgress::Complete(finalized) => {
            if expected_identifiers
                .iter()
                .any(|identifier| !finalized.identifiers.contains(identifier))
            {
                return Err(MidnightRuntimeError::ProofFailed);
            }
            state.wallet_state = proposed;
            return complete_json(finalized_transaction_result(&finalized)?);
        }
        transaction::BalanceProgress::Network(pending_request) => {
            let body = pending_request.body.clone();
            let effect = proof_effect(pending_request.kind);
            drop(state);
            let (operation, effect_id) = register_operation(
                session_id,
                generation,
                &session,
                PendingOperationKind::GenerateDustProof {
                    raw: transaction,
                    proposed_state: proposed,
                    expected_identifiers,
                    responses,
                    pending_request,
                },
            )?;
            return to_json(&OperationStep {
                kind: "network",
                operation: Some(operation),
                effect_id: Some(effect_id),
                effect: Some(effect),
                endpoint_role: Some("proof"),
                body_base64: Some(encode_base64(&body)),
                result_json: None,
            });
        }
    }
}
}
