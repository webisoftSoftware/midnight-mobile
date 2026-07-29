{
let balance_arguments = match &command {
    RuntimeCommand::BalanceUnsealed {
        raw_base64,
        ledger_parameters_base64,
        fee_blocks_margin,
        additional_fee_overhead,
    } => Some((
        false,
        raw_base64,
        ledger_parameters_base64,
        *fee_blocks_margin,
        additional_fee_overhead,
    )),
    RuntimeCommand::BalanceSealed {
        raw_base64,
        ledger_parameters_base64,
        fee_blocks_margin,
        additional_fee_overhead,
    } => Some((
        true,
        raw_base64,
        ledger_parameters_base64,
        *fee_blocks_margin,
        additional_fee_overhead,
    )),
    _ => None,
};

if let Some((
    original_sealed,
    raw_base64,
    ledger_parameters_base64,
    fee_blocks_margin,
    additional_fee_overhead,
)) = balance_arguments
{
    if !["shielded", "unshielded", "dust"]
        .iter()
        .all(|stream| state.caught_up_streams.contains(*stream))
    {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let original_raw = decode_base64(raw_base64)?;
    let parameters_raw = decode_base64(ledger_parameters_base64)?;
    let parameters = transaction::decode_ledger_parameters(&parameters_raw)?;
    if fee_blocks_margin > 64 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let fee_blocks_margin = usize::try_from(fee_blocks_margin)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let additional_fee_overhead_value = additional_fee_overhead
        .parse::<u128>()
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if additional_fee_overhead_value.to_string() != *additional_fee_overhead {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let erased = transaction::decode_balance_original(
        &original_raw,
        original_sealed,
        &state.config.network_id,
    )?;
    let current_time_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?
        .as_secs();
    let ttl_seconds = current_time_seconds
        .checked_add(30 * 60)
        .ok_or(MidnightRuntimeError::NativeInternal)?;
    let (_proposed, balancing_raw) =
        state.wallet_state.build_dust_balance(DustBalanceInput {
            network_id: &state.config.network_id,
            original: &erased,
            ledger_parameters: &parameters,
            fee_blocks_margin,
            additional_fee_overhead: additional_fee_overhead_value,
            dust_seed: &state.secrets.dust_seed,
            current_time_seconds,
            ttl_seconds,
        })?;
    let Some(balancing_raw) = balancing_raw else {
        let finalized = transaction::finalize_balance_original(
            &original_raw,
            original_sealed,
            &state.config.network_id,
        )?;
        return complete_json(finalized_transaction_result(&finalized)?);
    };
    let responses = transaction::RemoteProofResponses::default();
    let request = match transaction::advance_dust_balance(
        &original_raw,
        original_sealed,
        &balancing_raw,
        &state.config.network_id,
        &responses,
    )? {
        transaction::BalanceProgress::Network(request) => request,
        transaction::BalanceProgress::Complete(finalized) => {
            return complete_json(finalized_transaction_result(&finalized)?);
        }
    };
    drop(state);
    let step_body = request.body.clone();
    let step_effect = proof_effect(request.kind);
    let (operation, effect_id) = register_operation(
        session_id,
        generation,
        &session,
        PendingOperationKind::Balance {
            original_raw,
            original_sealed,
            balancing_raw,
            responses,
            pending_request: request,
        },
    )?;
    return to_json(&OperationStep {
        kind: "network",
        operation: Some(operation),
        effect_id: Some(effect_id),
        effect: Some(step_effect),
        endpoint_role: Some("proof"),
        body_base64: Some(encode_base64(&step_body)),
        result_json: None,
    });
}
}
