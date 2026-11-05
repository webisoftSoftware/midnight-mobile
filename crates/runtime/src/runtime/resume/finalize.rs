use super::super::*;

pub(super) fn resume(
    operation_id: u64,
    generation: u64,
    result: &NetworkResult,
    operation: PendingOperation,
    mut state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    match &operation.kind {
        PendingOperationKind::SubmitFinalized { transaction_hash } => {
            let submission = state
                .pending_submissions
                .get_mut(transaction_hash)
                .ok_or(MidnightRuntimeError::NativeInternal)?;
            match result.outcome.as_str() {
                "accepted" | "rejected" => {
                    let status = if result.outcome == "accepted" {
                        submission.status = SubmissionStatus::Accepted;
                        "accepted"
                    } else {
                        submission.status = SubmissionStatus::Rejected;
                        "rejected"
                    };
                    let identifiers = submission.identifiers.clone();
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
                        result_json: Some(submission_json(
                            transaction_hash,
                            identifiers,
                            status,
                            result,
                        )?),
                    })
                }
                "statusUnknown" => {
                    submission.status = SubmissionStatus::StatusUnknown;
                    drop(state);
                    lock_registry()?.operations.insert(operation_id, operation);
                    Err(MidnightRuntimeError::SubmissionStatusUnknown)
                }
                _ => {
                    drop(state);
                    lock_registry()?.operations.insert(operation_id, operation);
                    Err(MidnightRuntimeError::InvalidArgument)
                }
            }
        }
        PendingOperationKind::FinalizeTransactionBalance {
            proposed_state,
            expected_identifiers,
        } => match result.outcome.as_str() {
            "accepted" => {
                let decoded = decode_balance_service_result(
                    result,
                    &state.config.network_id,
                    expected_identifiers,
                );
                let (response, finalized, actual_hash) = match decoded {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        clear_active_operation(&mut state, operation_id);
                        return Err(error);
                    }
                };
                if let Some(proposed_state) = proposed_state {
                    state.wallet_state = proposed_state.clone();
                }
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
                    result_json: Some(balance_service_transaction_result(
                        &response,
                        &finalized,
                        &actual_hash,
                    )?),
                })
            }
            "rejected" | "statusUnknown" => {
                clear_active_operation(&mut state, operation_id);
                Err(MidnightRuntimeError::ProofFailed)
            }
            _ => {
                drop(state);
                lock_registry()?.operations.insert(operation_id, operation);
                Err(MidnightRuntimeError::InvalidArgument)
            }
        },
        PendingOperationKind::Extension(_)
        | PendingOperationKind::Balance { .. }
        | PendingOperationKind::FinalizeTransactionProof { .. }
        | PendingOperationKind::DappIntentProof { .. }
        | PendingOperationKind::GenerateDustProof { .. } => {
            Err(MidnightRuntimeError::NativeInternal)
        }
    }
}

// Submission result plus the node error, when the result carries one.
fn submission_json(
    transaction_hash: &str,
    identifiers: Vec<String>,
    status: &str,
    result: &NetworkResult,
) -> Result<String, MidnightRuntimeError> {
    let mut value = serde_json::from_str::<serde_json::Value>(&submission_result(
        transaction_hash,
        identifiers,
        status,
        result.body_base64.clone(),
    )?)
    .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if let Some(error) = &result.error {
        value["error"] = serde_json::json!(error);
    }
    to_json(&value)
}
