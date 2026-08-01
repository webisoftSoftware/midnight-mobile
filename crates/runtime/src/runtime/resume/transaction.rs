if matches!(
    &operation.kind,
    PendingOperationKind::FinalizeTransactionProof { .. }
) {
    match result.outcome.as_str() {
        "accepted" => {
            let response = result
                .body_base64
                .as_deref()
                .ok_or(MidnightRuntimeError::ProofFailed)
                .and_then(decode_base64);
            let response = match response {
                Ok(response) => response,
                Err(_) => {
                    clear_active_operation(&mut state, operation_id);
                    return Err(MidnightRuntimeError::ProofFailed);
                }
            };
            let progress = if let PendingOperationKind::FinalizeTransactionProof {
                raw,
                key_material,
                responses,
                pending_request,
                ..
            } = &mut operation.kind
            {
                if responses.accept(pending_request, response).is_err() {
                    clear_active_operation(&mut state, operation_id);
                    return Err(MidnightRuntimeError::ProofFailed);
                }
                transaction::advance_unproven_transaction_with_materials(
                    raw,
                    &state.config.network_id,
                    responses,
                    key_material,
                )
            } else {
                unreachable!()
            };
            match progress {
                Ok(transaction::BalanceProgress::Complete(finalized)) => {
                    let PendingOperationKind::FinalizeTransactionProof {
                        proposed_state,
                        expected_identifiers,
                        ..
                    } = operation.kind
                    else {
                        unreachable!()
                    };
                    if expected_identifiers
                        .iter()
                        .any(|identifier| !finalized.identifiers.contains(identifier))
                    {
                        clear_active_operation(&mut state, operation_id);
                        return Err(MidnightRuntimeError::ProofFailed);
                    }
                    operation.kind = PendingOperationKind::FinalizeTransactionBalance {
                        proposed_state,
                        expected_identifiers,
                    };
                    operation.effect_id =
                        next_effect_id(generation, operation_id, &operation.effect_id);
                    let effect_id = operation.effect_id.clone();
                    let body = Zeroizing::new(finalized.canonical);
                    drop(state);
                    lock_registry()?.operations.insert(operation_id, operation);
                    return to_json(&OperationStep {
                        kind: "network",
                        operation: Some(OperationHandle {
                            id: operation_id,
                            generation,
                        }),
                        effect_id: Some(effect_id),
                        effect: Some("balance"),
                        endpoint_role: Some("proof"),
                        body_base64: Some(encode_base64(&body)),
                        result_json: None,
                    });
                }
                Ok(transaction::BalanceProgress::Network(request)) => {
                    let body = Zeroizing::new(request.body.clone());
                    let effect = proof_effect(request.kind);
                    if let PendingOperationKind::FinalizeTransactionProof {
                        pending_request, ..
                    } = &mut operation.kind
                    {
                        *pending_request = request;
                    }
                    operation.effect_id =
                        next_effect_id(generation, operation_id, &operation.effect_id);
                    let effect_id = operation.effect_id.clone();
                    drop(state);
                    lock_registry()?.operations.insert(operation_id, operation);
                    return to_json(&OperationStep {
                        kind: "network",
                        operation: Some(OperationHandle {
                            id: operation_id,
                            generation,
                        }),
                        effect_id: Some(effect_id),
                        effect: Some(effect),
                        endpoint_role: Some("proof"),
                        body_base64: Some(encode_base64(&body)),
                        result_json: None,
                    });
                }
                Err(_) => {
                    clear_active_operation(&mut state, operation_id);
                    return Err(MidnightRuntimeError::ProofFailed);
                }
            }
        }
        "rejected" | "statusUnknown" => {
            clear_active_operation(&mut state, operation_id);
            return Err(MidnightRuntimeError::ProofFailed);
        }
        _ => {
            drop(state);
            lock_registry()?.operations.insert(operation_id, operation);
            return Err(MidnightRuntimeError::InvalidArgument);
        }
    }
}
