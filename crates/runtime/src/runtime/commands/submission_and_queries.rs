if let RuntimeCommand::SubmitFinalized { raw_base64 } = &command {
    let raw = decode_base64(raw_base64)?;
    let finalized = transaction::validate_finalized_transaction(&raw, &state.config.network_id)?;
    if state
        .pending_submissions
        .contains_key(&finalized.transaction_hash)
    {
        return Err(MidnightRuntimeError::SubmissionStatusUnknown);
    }
    drop(state);

    let transaction_hash = finalized.transaction_hash.clone();
    let (operation, effect_id) = register_operation(
        session_id,
        generation,
        &session,
        PendingOperationKind::SubmitFinalized {
            transaction_hash: transaction_hash.clone(),
        },
    )?;
    let mut state = match lock_session(&session) {
        Ok(state) => state,
        Err(error) => {
            discard_operation(operation.id);
            return Err(error);
        }
    };
    if state.generation != generation || state.closing {
        drop(state);
        discard_operation(operation.id);
        return Err(MidnightRuntimeError::StaleSession);
    }
    if state.pending_submissions.contains_key(&transaction_hash) {
        drop(state);
        discard_operation(operation.id);
        return Err(MidnightRuntimeError::SubmissionStatusUnknown);
    }
    state.pending_submissions.insert(
        transaction_hash.clone(),
        PendingSubmission {
            transaction_hash,
            identifiers: finalized.identifiers,
            status: SubmissionStatus::AwaitingResponse,
        },
    );
    return to_json(&OperationStep {
        kind: "network",
        operation: Some(operation),
        effect_id: Some(effect_id),
        effect: Some("submit"),
        endpoint_role: Some("node"),
        body_base64: Some(encode_base64(&finalized.canonical)),
        effects: None,
        result_json: None,
    });
}
