if matches!(
        &operation.kind,
        PendingOperationKind::DappIntentProof { .. }
    ) {
        let bodies = match decode_proof_batch(
            &results,
            operation.kind.pending_requests().unwrap_or(&[]),
            &operation.effect_id,
        ) {
            Ok(bodies) => bodies,
            Err(MidnightRuntimeError::InvalidArgument) => {
                drop(state);
                lock_registry()?.operations.insert(operation_id, operation);
                return Err(MidnightRuntimeError::InvalidArgument);
            }
            Err(error) => {
                clear_active_operation(&mut state, operation_id);
                return Err(error);
            }
        };
        let progress = if let PendingOperationKind::DappIntentProof {
            raw,
            responses,
            pending_requests,
        } = &mut operation.kind
        {
            if accept_proof_batch(responses, pending_requests, bodies).is_err() {
                clear_active_operation(&mut state, operation_id);
                return Err(MidnightRuntimeError::ProofFailed);
            }
            transaction::advance_unproven_transaction(raw, &state.config.network_id, responses)
        } else {
            unreachable!()
        };
        match progress {
            Ok(transaction::BalanceProgress::Complete(finalized)) => {
                clear_active_operation(&mut state, operation_id);
                return to_json(&OperationStep {
                    kind: "complete",
                    operation: Some(OperationHandle {
                        id: operation_id,
                        generation,
                    }),
                    effect_id: None,
                    effect: None,
                    endpoint_role: None,
                    body_base64: None,
                    effects: None,
                    result_json: Some(finalized_transaction_result(&finalized)?),
                });
            }
            Ok(transaction::BalanceProgress::Network(requests)) => {
                operation.kind.set_pending_requests(requests);
                operation.effect_id =
                    next_effect_id(generation, operation_id, &operation.effect_id);
                let step = pending_proof_step(&operation, operation_id, generation)?;
                drop(state);
                lock_registry()?.operations.insert(operation_id, operation);
                return to_json(&step);
            }
            Err(_) => {
                clear_active_operation(&mut state, operation_id);
                return Err(MidnightRuntimeError::ProofFailed);
            }
        }
}
