{
if let RuntimeCommand::DappTransfer { outputs } = &mut command {
    if !["shielded", "unshielded", "dust"]
        .iter()
        .all(|stream| state.caught_up_streams.contains(*stream))
    {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let outputs = parse_dapp_outputs(std::mem::take(outputs))?;
    let ttl_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?
        .as_secs()
        .checked_add(30 * 60)
        .ok_or(MidnightRuntimeError::NativeInternal)?;
    let (proposed, transaction) = state.wallet_state.build_dapp_transfer_with_rng(
        &state.config.network_id,
        &outputs,
        &state.secrets.night_external_key,
        &state.secrets.zswap_seed,
        ttl_seconds,
        &mut OsRng,
    )?;
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

if let RuntimeCommand::DappIntent { inputs, outputs } = &mut command {
    if !["shielded", "unshielded", "dust"]
        .iter()
        .all(|stream| state.caught_up_streams.contains(*stream))
    {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let inputs = parse_dapp_inputs(std::mem::take(inputs))?;
    let outputs = parse_dapp_outputs(std::mem::take(outputs))?;
    let ttl_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?
        .as_secs()
        .checked_add(30 * 60)
        .ok_or(MidnightRuntimeError::NativeInternal)?;
    let raw = state.wallet_state.build_dapp_intent_with_rng(
        DappIntentBuildInput {
            network_id: &state.config.network_id,
            inputs: &inputs,
            outputs: &outputs,
            night_external_key: &state.secrets.night_external_key,
            zswap_seed: &state.secrets.zswap_seed,
            ttl_seconds,
        },
        &mut OsRng,
    )?;
    let responses = transaction::RemoteProofResponses::default();
    let progress =
        transaction::advance_unproven_transaction(&raw, &state.config.network_id, &responses)?;
    match progress {
        transaction::BalanceProgress::Complete(finalized) => {
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
                PendingOperationKind::DappIntentProof {
                    raw,
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
