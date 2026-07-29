{
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
                        result_json: Some(submission_result(
                            transaction_hash,
                            identifiers,
                            status,
                            result.body_base64.clone(),
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
        PendingOperationKind::Transfer {
            proposed_state,
            expected_identifiers,
        } => match result.outcome.as_str() {
            "accepted" => {
                let decoded = decode_balance_service_result(
                    &result,
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
                state.wallet_state = proposed_state.clone();
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
        PendingOperationKind::Balance { .. } => unreachable!("balance handled above"),
        PendingOperationKind::DappIntentProof { .. } => {
            unreachable!("dapp intent proof handled above")
        }
        PendingOperationKind::GenerateDustProof { .. } => {
            unreachable!("DUST registration proof handled above")
        }
    }
}
