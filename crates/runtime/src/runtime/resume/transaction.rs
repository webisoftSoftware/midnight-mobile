if matches!(
    &operation.kind,
    PendingOperationKind::FinalizeTransactionProof { .. }
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
    let progress = if let PendingOperationKind::FinalizeTransactionProof {
        raw,
        key_material,
        responses,
        pending_requests,
        ..
    } = &mut operation.kind
    {
        if accept_proof_batch(responses, pending_requests, bodies).is_err() {
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
            operation.effect_id = next_effect_id(generation, operation_id, &operation.effect_id);
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
                effects: None,
                result_json: None,
            });
        }
        Ok(transaction::BalanceProgress::Network(requests)) => {
            operation.kind.set_pending_requests(requests);
            operation.effect_id = next_effect_id(generation, operation_id, &operation.effect_id);
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
