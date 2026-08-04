use super::*;

#[test]
fn one_active_operation_blocks_another_operation_and_sync() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("single-operation", None);
    let (operation, _, _) = install_submission_operation(&handle, 1);
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    assert_runtime_error(
        register_operation(
            handle.id,
            handle.generation,
            &session,
            PendingOperationKind::SubmitFinalized {
                transaction_hash: format!("{:064x}", 2),
            },
        ),
        "UNAVAILABLE",
    );
    assert_runtime_error(
        apply_sync_batch(
            handle.id,
            handle.generation,
            "unshielded-tip".to_owned(),
            0,
            0,
            Vec::new(),
        ),
        "UNAVAILABLE",
    );
    cancel_operation(operation.id, operation.generation).unwrap();
    apply_sync_batch(
        handle.id,
        handle.generation,
        "unshielded-tip".to_owned(),
        0,
        0,
        Vec::new(),
    )
    .unwrap();
}

#[test]
fn transaction_and_dust_reservations_block_sync_and_cleanup() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("transaction-reservation", None);
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    {
        let mut state = lock_session(&session).unwrap();
        reserve_operation(&mut state).unwrap();
    }
    assert_runtime_error(
        apply_sync_batch(
            handle.id,
            handle.generation,
            "unshielded-tip".to_owned(),
            0,
            0,
            Vec::new(),
        ),
        "UNAVAILABLE",
    );
    assert_runtime_error(
        start_transaction_finalization(
            handle.id,
            handle.generation,
            &session,
            TransactionFinalizationInput {
                network_id: "preview".to_owned(),
                raw: vec![0xff],
                key_material: transaction::RemoteProofKeyMaterials::default(),
                proposed_state: None,
                expected_identifiers: Vec::new(),
            },
        ),
        "INVALID_ARGUMENT",
    );
    assert!(lock_session(&session).unwrap().active_operation.is_none());
    apply_sync_batch(
        handle.id,
        handle.generation,
        "unshielded-tip".to_owned(),
        0,
        0,
        Vec::new(),
    )
    .unwrap();

    let proposed_state = lock_session(&session).unwrap().wallet_state.clone();
    {
        let mut state = lock_session(&session).unwrap();
        reserve_operation(&mut state).unwrap();
    }
    assert_runtime_error(
        apply_sync_batch(
            handle.id,
            handle.generation,
            "shielded-tip".to_owned(),
            0,
            0,
            Vec::new(),
        ),
        "UNAVAILABLE",
    );
    let (operation, _) = register_reserved_operation(
        handle.id,
        handle.generation,
        &session,
        PendingOperationKind::GenerateDustProof {
            raw: vec![1],
            proposed_state,
            expected_identifiers: Vec::new(),
            responses: transaction::RemoteProofResponses::default(),
            pending_requests: vec![transaction::RemoteProofRequest {
                key: "dust-reservation".to_owned(),
                kind: transaction::RemoteProofKind::Check,
                body: vec![2],
            }],
        },
    )
    .unwrap();
    assert_eq!(
        lock_session(&session).unwrap().active_operation,
        Some(operation.id)
    );
    assert_runtime_error(
        apply_sync_batch(
            handle.id,
            handle.generation,
            "shielded-tip".to_owned(),
            0,
            0,
            Vec::new(),
        ),
        "UNAVAILABLE",
    );
    cancel_operation(operation.id, operation.generation).unwrap();
    apply_sync_batch(
        handle.id,
        handle.generation,
        "shielded-tip".to_owned(),
        0,
        0,
        Vec::new(),
    )
    .unwrap();
}
