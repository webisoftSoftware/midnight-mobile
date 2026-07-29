static TEST_RUNTIME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct IsolatedRuntime {
    _serial: MutexGuard<'static, ()>,
}

impl Drop for IsolatedRuntime {
    fn drop(&mut self) {
        let mut runtime = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *runtime = RuntimeRegistry::default();
    }
}

fn isolated_runtime() -> IsolatedRuntime {
    let serial = TEST_RUNTIME_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    {
        let mut runtime = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *runtime = RuntimeRegistry::default();
    }
    IsolatedRuntime { _serial: serial }
}

fn test_config(wallet_fingerprint: &str) -> String {
    serde_json::json!({
        "networkId": "preview",
        "walletFingerprint": wallet_fingerprint,
        "unshieldedAddress": "synthetic-address",
    })
    .to_string()
}

fn open_test_session(
    wallet_fingerprint: &str,
    checkpoint: Option<Vec<u8>>,
) -> RuntimeSessionHandle {
    test_session_result(wallet_fingerprint, checkpoint).unwrap()
}

fn test_session_result(
    wallet_fingerprint: &str,
    checkpoint: Option<Vec<u8>>,
) -> Result<RuntimeSessionHandle, MidnightRuntimeError> {
    open_wallet_session(
        test_config(wallet_fingerprint),
        vec![1; 32],
        vec![2; 32],
        vec![3; 32],
        checkpoint,
    )
}

fn assert_runtime_error<T: std::fmt::Debug>(
    result: Result<T, MidnightRuntimeError>,
    expected: &str,
) {
    assert_eq!(result.unwrap_err().to_string(), expected);
}

fn install_submission_operation(
    handle: &RuntimeSessionHandle,
    marker: u8,
) -> (OperationHandle, String, String) {
    let transaction_hash = format!("{marker:064x}");
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    {
        let mut state = lock_session(&session).unwrap();
        state.pending_submissions.insert(
            transaction_hash.clone(),
            PendingSubmission {
                transaction_hash: transaction_hash.clone(),
                identifiers: vec![format!("{:02x}", marker.saturating_add(1))],
                status: SubmissionStatus::AwaitingResponse,
            },
        );
    }
    let (operation, effect_id) = register_operation(
        handle.id,
        handle.generation,
        &session,
        PendingOperationKind::SubmitFinalized {
            transaction_hash: transaction_hash.clone(),
        },
    )
    .unwrap();
    (operation, effect_id, transaction_hash)
}

fn pending_status(handle: &RuntimeSessionHandle, transaction_hash: &str) -> String {
    let raw = get_wallet_snapshot(handle.id, handle.generation).unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&raw).unwrap();
    snapshot["pendingSubmissions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|submission| submission["transactionHash"] == transaction_hash)
        .and_then(|submission| submission["status"].as_str())
        .unwrap()
        .to_owned()
}

#[test]
fn begin_command_rejects_removed_unknown_and_malformed_commands_before_execution() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("command-boundary", None);
    let snapshot_before = get_wallet_snapshot(handle.id, handle.generation).unwrap();
    let removed_kind = format!("sign{}Challenge", ["Gate", "way"].concat());
    let commands = [
        serde_json::json!({"kind": removed_kind, "dataBase64": ""}).to_string(),
        r#"{"kind":"futureCommand"}"#.to_owned(),
        r#"{"kind":"signData","dataBase64":""}"#.to_owned(),
        r#"{"kind":"signData","domain":"test","dataBase64":"","extra":true}"#.to_owned(),
        "{".to_owned(),
    ];

    for command in commands {
        assert_runtime_error(
            begin_command(handle.id, handle.generation, command),
            "INVALID_ARGUMENT",
        );
    }
    assert_eq!(
        get_wallet_snapshot(handle.id, handle.generation).unwrap(),
        snapshot_before
    );
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    assert!(lock_session(&session).unwrap().active_operation.is_none());
}

#[test]
fn signing_transcript_separates_version_domain_and_length_boundaries() {
    let transcript = signing_transcript("ab", b"c").unwrap();
    let mut expected = b"midnight-mobile/sign-data".to_vec();
    expected.push(1);
    expected.extend_from_slice(&2_u64.to_be_bytes());
    expected.extend_from_slice(b"ab");
    expected.extend_from_slice(&1_u64.to_be_bytes());
    expected.extend_from_slice(b"c");
    assert_eq!(transcript, expected);
    assert_ne!(
        signing_transcript("ab", b"c").unwrap(),
        signing_transcript("a", b"bc").unwrap()
    );
    assert_ne!(
        signing_transcript("domain-a", b"payload").unwrap(),
        signing_transcript("domain-b", b"payload").unwrap()
    );
    assert_runtime_error(signing_transcript("", b"payload"), "INVALID_ARGUMENT");
    assert_runtime_error(
        signing_transcript(&"d".repeat(MAX_SIGNING_DOMAIN_BYTES + 1), b"payload"),
        "INVALID_ARGUMENT",
    );
    assert_runtime_error(
        signing_transcript("domain", &vec![0; MAX_DAPP_SIGN_DATA_BYTES + 1]),
        "INVALID_ARGUMENT",
    );
}

#[test]
fn checkpoints_reject_tampering_and_restore_into_a_new_generation() {
    let _runtime = isolated_runtime();
    let original = open_test_session("checkpoint-wallet", None);
    let checkpoint = export_wallet_checkpoint(original.id, original.generation).unwrap();
    assert_eq!(&checkpoint[..4], CHECKPOINT_MAGIC);
    assert_eq!(
        u32::from_be_bytes(checkpoint[4..8].try_into().unwrap()),
        CHECKPOINT_VERSION
    );

    let mut tampered = decode_checkpoint(&checkpoint).unwrap();
    tampered.wallet_fingerprint.push_str("-tampered");
    let tampered = encode_checkpoint(&tampered).unwrap();
    assert_runtime_error(
        test_session_result("checkpoint-wallet", Some(tampered)),
        "STATE_INCOMPATIBLE",
    );
    close_wallet_session(original.id, original.generation).unwrap();
    let restored = open_test_session("checkpoint-wallet", Some(checkpoint));
    assert!(restored.generation > original.generation);
    assert_runtime_error(
        get_wallet_snapshot(original.id, original.generation),
        "STALE_SESSION",
    );
}

#[test]
fn checkpoint_restore_rejects_unknown_versions_and_identity_mismatches() {
    let _runtime = isolated_runtime();
    let original = open_test_session("checkpoint-identity", None);
    let checkpoint = export_wallet_checkpoint(original.id, original.generation).unwrap();
    close_wallet_session(original.id, original.generation).unwrap();

    let mut unknown_version = checkpoint.clone();
    unknown_version[4..8].copy_from_slice(&(CHECKPOINT_VERSION + 1).to_be_bytes());
    assert_runtime_error(
        test_session_result("checkpoint-identity", Some(unknown_version)),
        "STATE_INCOMPATIBLE",
    );
    assert_runtime_error(
        test_session_result("different-wallet", Some(checkpoint)),
        "STATE_INCOMPATIBLE",
    );
}

#[test]
fn runtime_defaults_to_two_sessions_and_releases_capacity_on_close() {
    let _runtime = isolated_runtime();
    let first = open_test_session("session-one", None);
    let second = open_test_session("session-two", None);
    assert_runtime_error(
        test_session_result("session-three", None),
        "UNAVAILABLE",
    );
    close_wallet_session(first.id, first.generation).unwrap();
    let third = open_test_session("session-three", None);
    close_wallet_session(second.id, second.generation).unwrap();
    close_wallet_session(third.id, third.generation).unwrap();
}

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
fn stale_generations_cannot_close_cancel_resume_or_read_live_state() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("generation-fence", None);
    let (operation, effect_id, _) = install_submission_operation(&handle, 3);
    let stale_generation = handle.generation + 1;
    let network_result =
        serde_json::json!({"effectId": effect_id, "outcome": "accepted"}).to_string();

    assert_runtime_error(
        get_wallet_snapshot(handle.id, stale_generation),
        "STALE_SESSION",
    );
    assert_runtime_error(
        close_wallet_session(handle.id, stale_generation),
        "STALE_SESSION",
    );
    assert_runtime_error(
        cancel_operation(operation.id, stale_generation),
        "STALE_SESSION",
    );
    assert_runtime_error(
        resume_operation(operation.id, stale_generation, Some(network_result)),
        "STALE_SESSION",
    );
    cancel_operation(operation.id, handle.generation).unwrap();
    close_wallet_session(handle.id, handle.generation).unwrap();
    assert_runtime_error(
        get_wallet_snapshot(handle.id, handle.generation),
        "STALE_SESSION",
    );
}

#[test]
fn cancellation_releases_the_session_and_consumes_a_bounded_tombstone() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("cancellation", None);
    let (operation, effect_id, _) = install_submission_operation(&handle, 4);
    cancel_operation(operation.id, operation.generation).unwrap();
    assert_runtime_error(
        cancel_operation(operation.id, operation.generation),
        "CANCELLED",
    );
    let result =
        serde_json::json!({"effectId": effect_id, "outcome": "accepted"}).to_string();
    assert_runtime_error(
        resume_operation(
            operation.id,
            operation.generation,
            Some(result.clone()),
        ),
        "CANCELLED",
    );
    assert_runtime_error(
        resume_operation(operation.id, operation.generation, Some(result)),
        "STALE_SESSION",
    );
    apply_sync_batch(
        handle.id,
        handle.generation,
        "dust-tip".to_owned(),
        0,
        0,
        Vec::new(),
    )
    .unwrap();
}

#[test]
fn sync_requires_contiguous_offsets_and_replays_receipts_idempotently() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("sync-fencing", None);
    let first =
        br#"{"type":"UnshieldedTransactionsProgress","highestTransactionId":1}"#.to_vec();
    let second =
        br#"{"type":"UnshieldedTransactionsProgress","highestTransactionId":2}"#.to_vec();
    let applied = apply_sync_batch(
        handle.id,
        handle.generation,
        "unshielded".to_owned(),
        0,
        1,
        vec![first.clone()],
    )
    .unwrap();
    assert!(!serde_json::from_str::<serde_json::Value>(&applied).unwrap()["duplicate"]
        .as_bool()
        .unwrap());
    let duplicate = apply_sync_batch(
        handle.id,
        handle.generation,
        "unshielded".to_owned(),
        0,
        1,
        vec![first],
    )
    .unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&duplicate).unwrap()["duplicate"]
            .as_bool()
            .unwrap()
    );
    assert_runtime_error(
        apply_sync_batch(
            handle.id,
            handle.generation,
            "unshielded".to_owned(),
            0,
            1,
            vec![second.clone()],
        ),
        "SYNC_GAP",
    );
    assert_runtime_error(
        apply_sync_batch(
            handle.id,
            handle.generation,
            "unshielded".to_owned(),
            3,
            4,
            vec![second.clone()],
        ),
        "SYNC_GAP",
    );
    apply_sync_batch(
        handle.id,
        handle.generation,
        "unshielded".to_owned(),
        1,
        2,
        vec![second],
    )
    .unwrap();
}

#[test]
fn ambiguous_and_cancelled_submissions_remain_status_unknown() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("submission-status", None);
    let (ambiguous, effect_id, ambiguous_hash) = install_submission_operation(&handle, 5);
    let ambiguous_result =
        serde_json::json!({"effectId": effect_id, "outcome": "statusUnknown"}).to_string();
    assert_runtime_error(
        resume_operation(
            ambiguous.id,
            ambiguous.generation,
            Some(ambiguous_result),
        ),
        "SUBMISSION_STATUS_UNKNOWN",
    );
    assert_eq!(
        pending_status(&handle, &ambiguous_hash),
        "statusUnknown".to_owned()
    );
    cancel_operation(ambiguous.id, ambiguous.generation).unwrap();

    let (cancelled, cancelled_effect, cancelled_hash) =
        install_submission_operation(&handle, 6);
    assert_eq!(
        pending_status(&handle, &cancelled_hash),
        "awaitingResponse".to_owned()
    );
    cancel_operation(cancelled.id, cancelled.generation).unwrap();
    assert_eq!(
        pending_status(&handle, &cancelled_hash),
        "statusUnknown".to_owned()
    );
    let cancelled_result =
        serde_json::json!({"effectId": cancelled_effect, "outcome": "accepted"}).to_string();
    assert_runtime_error(
        resume_operation(
            cancelled.id,
            cancelled.generation,
            Some(cancelled_result),
        ),
        "CANCELLED",
    );
}

#[test]
fn exhausted_runtime_counters_fail_closed_without_reusing_handles() {
    let _runtime = isolated_runtime();
    {
        let mut runtime = lock_registry().unwrap();
        runtime.next_generation = u64::MAX;
    }
    assert_runtime_error(
        test_session_result("generation-exhausted", None),
        "UNAVAILABLE",
    );
    assert!(lock_registry().unwrap().sessions.is_empty());

    {
        let mut runtime = lock_registry().unwrap();
        runtime.next_generation = 1;
        runtime.next_id = u64::MAX;
    }
    assert_runtime_error(
        test_session_result("identifier-exhausted", None),
        "UNAVAILABLE",
    );
    assert!(lock_registry().unwrap().sessions.is_empty());

    {
        let mut runtime = lock_registry().unwrap();
        runtime.next_id = 1;
    }
    let handle = open_test_session("operation-exhausted", None);
    {
        let mut runtime = lock_registry().unwrap();
        runtime.next_id = u64::MAX;
    }
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    assert_runtime_error(
        register_operation(
            handle.id,
            handle.generation,
            &session,
            PendingOperationKind::SubmitFinalized {
                transaction_hash: format!("{:064x}", 7),
            },
        ),
        "UNAVAILABLE",
    );
    assert!(lock_session(&session).unwrap().active_operation.is_none());
}
