use super::super::*;

pub(super) fn resume(
    operation_id: u64,
    generation: u64,
    results: &[NetworkResult],
    mut operation: PendingOperation,
    mut state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    let bodies = match decode_proof_batch(
        results,
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
        return Err(MidnightRuntimeError::NativeInternal);
    };
    match progress {
        Ok(transaction::BalanceProgress::Complete(finalized)) => {
            let PendingOperationKind::FinalizeTransactionProof {
                proposed_state,
                expected_identifiers,
                local_balance,
                ..
            } = operation.kind
            else {
                return Err(MidnightRuntimeError::NativeInternal);
            };
            if !covers_expected_identifiers(&finalized, &expected_identifiers) {
                clear_active_operation(&mut state, operation_id);
                return Err(MidnightRuntimeError::ProofFailed);
            }
            if let Some(request) = local_balance {
                return local_balance_step(
                    operation_id,
                    generation,
                    state,
                    LocalBalanceStep {
                        session_id: operation.session_id,
                        effect_id: operation.effect_id,
                        request,
                        proposed_state,
                        expected_identifiers,
                        finalized,
                    },
                );
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
            to_json(&OperationStep {
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
            })
        }
        Ok(transaction::BalanceProgress::Network(requests)) => {
            operation.kind.set_pending_requests(requests);
            operation.effect_id = next_effect_id(generation, operation_id, &operation.effect_id);
            let step = pending_proof_step(&operation, operation_id, generation)?;
            drop(state);
            lock_registry()?.operations.insert(operation_id, operation);
            to_json(&step)
        }
        Err(_) => {
            clear_active_operation(&mut state, operation_id);
            Err(MidnightRuntimeError::ProofFailed)
        }
    }
}

struct LocalBalanceStep {
    session_id: u64,
    effect_id: String,
    request: LocalDustBalanceRequest,
    proposed_state: Option<NativeWalletState>,
    expected_identifiers: Vec<String>,
    finalized: transaction::FinalizedTransaction,
}

// Pays DUST fees locally after proving; either completes or proves the balancing part.
fn local_balance_step(
    operation_id: u64,
    generation: u64,
    mut state: MutexGuard<'_, SessionState>,
    step: LocalBalanceStep,
) -> Result<String, MidnightRuntimeError> {
    let LocalBalanceStep {
        session_id,
        effect_id,
        request,
        proposed_state,
        expected_identifiers,
        finalized,
    } = step;
    let start = proposed_state
        .ok_or(MidnightRuntimeError::NativeInternal)
        .and_then(|proposed| begin_local_dust_balance(&state, &request, proposed, finalized));
    let start = match start {
        Ok(start) => start,
        Err(error) => {
            clear_active_operation(&mut state, operation_id);
            return Err(error);
        }
    };
    match start {
        LocalDustBalanceStart::Complete {
            finalized,
            proposed_state,
        } => {
            if !covers_expected_identifiers(&finalized, &expected_identifiers) {
                clear_active_operation(&mut state, operation_id);
                return Err(MidnightRuntimeError::ProofFailed);
            }
            state.wallet_state = proposed_state;
            clear_active_operation(&mut state, operation_id);
            to_json(&OperationStep {
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
            })
        }
        LocalDustBalanceStart::Pending {
            original_raw,
            balancing_raw,
            responses,
            pending_requests,
            proposed_state,
        } => {
            let operation = PendingOperation {
                generation,
                session_id,
                effect_id: next_effect_id(generation, operation_id, &effect_id),
                kind: PendingOperationKind::Balance {
                    original_raw,
                    original_sealed: true,
                    balancing_raw,
                    responses,
                    pending_requests,
                    proposed_state: Some(proposed_state),
                    expected_identifiers,
                },
            };
            let step = pending_proof_step(&operation, operation_id, generation)?;
            drop(state);
            lock_registry()?.operations.insert(operation_id, operation);
            to_json(&step)
        }
    }
}
