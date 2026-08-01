fn run_command(
    handle: &RuntimeSessionHandle,
    command: serde_json::Value,
) -> Result<serde_json::Value, MidnightRuntimeError> {
    begin_command(handle.id, handle.generation, command.to_string())
        .and_then(|value| serde_json::from_str(&value).map_err(|_| MidnightRuntimeError::NativeInternal))
}

fn resume_result(
    operation: &OperationHandle,
    effect_id: &str,
    outcome: &str,
) -> Result<String, MidnightRuntimeError> {
    resume_operation(
        operation.id,
        operation.generation,
        Some(
            serde_json::json!({
                "effectId": effect_id,
                "outcome": outcome
            })
            .to_string(),
        ),
    )
}

fn valid_checkpoint_fixture() -> WalletCheckpoint {
    let handle = open_test_session("coverage-checkpoint", None);
    let encoded = export_wallet_checkpoint(handle.id, handle.generation).unwrap();
    close_wallet_session(handle.id, handle.generation).unwrap();
    decode_checkpoint(&encoded).unwrap()
}

#[test]
fn runtime_codecs_and_operation_result_helpers_cover_wire_shapes() {
    for bytes in [
        Vec::new(),
        vec![0],
        vec![0, 1],
        vec![0, 1, 2],
        (0_u8..=63).collect(),
    ] {
        assert_eq!(decode_base64(&encode_base64(&bytes)).unwrap(), bytes);
    }
    for invalid in ["A", "AAA", "=AAA", "A===", "AA=A", "AA==A", "!!!!"] {
        assert!(decode_base64(invalid).is_err(), "{invalid}");
    }

    assert_eq!(next_effect_id(2, 3, "2:3:9"), "2:3:10");
    assert_eq!(next_effect_id(2, 3, "malformed"), "2:3:1");
    assert_eq!(
        normalize_transaction_hash(&format!(" 0x{} ", "AB".repeat(32))).unwrap(),
        "ab".repeat(32)
    );
    assert_eq!(
        normalize_transaction_hash(&format!("shielded-{}", "cd".repeat(32))).unwrap(),
        "cd".repeat(32)
    );
    assert!(normalize_transaction_hash("not-a-hash").is_none());

    let inputs = parse_dapp_inputs(vec![RuntimeDappInput {
        wallet_type: "shielded".to_owned(),
        token_type: "token".to_owned(),
        amount: "12".to_owned(),
    }])
    .unwrap();
    assert_eq!(inputs[0].amount, 12);
    assert!(
        parse_dapp_inputs(vec![RuntimeDappInput {
            wallet_type: "shielded".to_owned(),
            token_type: "token".to_owned(),
            amount: "-1".to_owned(),
        }])
        .is_err()
    );
    assert!(
        parse_dapp_outputs(vec![RuntimeDappOutput {
            wallet_type: "unshielded".to_owned(),
            token_type: "token".to_owned(),
            amount: "3".to_owned(),
            receiver_address: "receiver".to_owned(),
        }])
        .is_ok()
    );
    assert_eq!(proof_effect(transaction::RemoteProofKind::Check), "check");
    assert_eq!(proof_effect(transaction::RemoteProofKind::Prove), "prove");

    let mut none = None;
    assert!(take_proving_key_material(&mut none).unwrap().is_none());
    let mut material = Some(RuntimeProvingKeyMaterial {
        prover_key_base64: "AA==".to_owned(),
        verifier_key_base64: "AQ==".to_owned(),
        ir_base64: "Ag==".to_owned(),
        compression: None,
    });
    let decoded = take_proving_key_material(&mut material).unwrap().unwrap();
    assert_eq!(decoded.prover_key, vec![0]);
    let mut invalid = Some(RuntimeProvingKeyMaterial {
        prover_key_base64: "AA==".to_owned(),
        verifier_key_base64: "AA==".to_owned(),
        ir_base64: "AA==".to_owned(),
        compression: Some("other".to_owned()),
    });
    assert!(take_proving_key_material(&mut invalid).is_err());

    let finalized = transaction::FinalizedTransaction {
        canonical: vec![1, 2, 3],
        transaction_hash: "ab".repeat(32),
        identifiers: vec!["cd".repeat(32)],
    };
    assert!(serde_json::from_str::<serde_json::Value>(
        &finalized_transaction_result(&finalized).unwrap()
    )
    .unwrap()["transactionBase64"]
        .is_string());
    let response = BalanceServiceResult {
        tx_hash: "ef".repeat(32),
        tx_bytes: "010203".to_owned(),
        expires_at: Some(7),
    };
    let balance = balance_service_transaction_result(&response, &finalized, "ef").unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&balance).unwrap()["expiresAt"],
        7
    );
    let submission =
        submission_result("aa", vec!["bb".to_owned()], "accepted", Some("AA==".to_owned()))
            .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&submission).unwrap()["bodyBase64"],
        "AA=="
    );
}

#[test]
fn checkpoint_validation_rejects_each_structural_drift() {
    let _runtime = isolated_runtime();
    let base = valid_checkpoint_fixture();
    assert!(valid_lower_hex(&"ab".repeat(32), 64));
    assert!(!valid_lower_hex(&"AB".repeat(32), 64));
    assert!(valid_variable_lower_hex("00"));
    assert!(!valid_variable_lower_hex(""));
    assert!(!valid_variable_lower_hex("0"));

    let mut variants = Vec::new();
    let mut invalid_stream = base.clone();
    invalid_stream.stream_offsets.push(StreamOffset {
        stream: "private".to_owned(),
        next_offset: 0,
    });
    variants.push(invalid_stream);
    let mut duplicate_stream = base.clone();
    duplicate_stream.stream_offsets = vec![
        StreamOffset {
            stream: "dust".to_owned(),
            next_offset: 0,
        },
        StreamOffset {
            stream: "dust".to_owned(),
            next_offset: 0,
        },
    ];
    variants.push(duplicate_stream);
    let mut duplicate_caught_up = base.clone();
    duplicate_caught_up.caught_up_streams = vec!["dust".to_owned(), "dust".to_owned()];
    variants.push(duplicate_caught_up);
    let mut bad_receipt = base.clone();
    bad_receipt.stream_offsets.push(StreamOffset {
        stream: "dust".to_owned(),
        next_offset: 1,
    });
    bad_receipt.batch_receipts.push(BatchReceipt {
        stream: "dust".to_owned(),
        from_offset: 0,
        to_offset: 1,
        digest: "GG".repeat(32),
    });
    variants.push(bad_receipt);
    let mut bad_submission = base.clone();
    bad_submission.pending_submissions.push(PendingSubmission {
        transaction_hash: "AA".repeat(32),
        identifiers: vec!["00".to_owned()],
        status: SubmissionStatus::Accepted,
    });
    variants.push(bad_submission);
    for mut checkpoint in variants {
        checkpoint.checksum = checkpoint_checksum(&checkpoint);
        assert!(validate_checkpoint(checkpoint).is_err());
    }

    let mut complete = base;
    complete.pending_submissions = vec![
        PendingSubmission {
            transaction_hash: "01".repeat(32),
            identifiers: vec!["02".repeat(2)],
            status: SubmissionStatus::AwaitingResponse,
        },
        PendingSubmission {
            transaction_hash: "03".repeat(32),
            identifiers: vec!["04".repeat(2)],
            status: SubmissionStatus::Accepted,
        },
        PendingSubmission {
            transaction_hash: "05".repeat(32),
            identifiers: vec!["06".repeat(2)],
            status: SubmissionStatus::Rejected,
        },
        PendingSubmission {
            transaction_hash: "07".repeat(32),
            identifiers: vec!["08".repeat(2)],
            status: SubmissionStatus::StatusUnknown,
        },
    ];
    complete.checksum = checkpoint_checksum(&complete);
    let encoded = encode_checkpoint(&complete).unwrap();
    assert_eq!(decode_checkpoint(&encoded).unwrap().pending_submissions.len(), 4);

    assert_ne!(payload_digest(&[vec![1], vec![2]]), payload_digest(&[vec![1, 2]]));
    let mut receipts = HashMap::new();
    for index in 0..=MAX_SYNC_BATCH_RECEIPTS {
        receipts.insert(
            BatchKey {
                stream: "dust".to_owned(),
                from_offset: index as u64,
                to_offset: index as u64,
            },
            "00".repeat(32),
        );
    }
    prune_batch_receipts(&mut receipts);
    assert_eq!(receipts.len(), MAX_SYNC_BATCH_RECEIPTS);
}

#[test]
fn wallet_commands_cover_sync_codec_and_validation_paths() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("coverage-commands", None);
    let sync = run_command(
        &handle,
        serde_json::json!({
            "kind": "createSyncRequest",
            "stream": "shielded",
            "fromOffset": 0,
            "limit": 10
        }),
    )
    .unwrap();
    assert_eq!(sync["result"]["stream"], "shielded");
    let accelerated = run_command(
        &handle,
        serde_json::json!({
            "kind": "createSyncRequest",
            "mode": "fast",
            "stream": "shielded",
            "fromOffset": 0
        }),
    )
    .unwrap();
    let request = decode_base64(accelerated["result"]["requestBase64"].as_str().unwrap()).unwrap();
    let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(request["coinPublicKey"].as_str().unwrap().len(), 64);
    assert!(
        run_command(
            &handle,
            serde_json::json!({
                "kind": "createSyncRequest",
                "mode": "fast",
                "stream": "shielded",
                "fromOffset": 1
            }),
        )
        .is_err()
    );
    let mint_context =
        run_command(&handle, serde_json::json!({"kind": "deriveShieldedMintContext"})).unwrap();
    assert_eq!(
        mint_context["result"]["coinPublicKeyHex"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(
        run_command(
            &handle,
            serde_json::json!({
                "kind": "createSyncRequest",
                "stream": "bad",
                "fromOffset": 0,
                "limit": 10
            }),
        )
        .is_err()
    );
    assert!(run_command(&handle, serde_json::json!({"kind": "createShieldedSpentRequest"})).is_ok());
    assert!(
        run_command(
            &handle,
            serde_json::json!({
                "kind": "applyShieldedSpentResponse",
                "resultBase64": encode_base64(br#"{"results":[]}"#)
            }),
        )
        .is_ok()
    );
    assert!(
        run_command(
            &handle,
            serde_json::json!({
                "kind": "setShieldedProtocolVersion",
                "protocolVersion": 8,
                "syncOffset": 0
            }),
        )
        .is_ok()
    );
    assert!(run_command(&handle, serde_json::json!({"kind": "createDustSpendRequest"})).is_ok());
    let mut spend = Vec::new();
    spend.extend_from_slice(&1_u64.to_le_bytes());
    spend.extend_from_slice(&0_u32.to_le_bytes());
    assert!(
        run_command(
            &handle,
            serde_json::json!({
                "kind": "createDustCommitmentRequest",
                "syncOffset": 0,
                "rawBase64": encode_base64(&spend)
            }),
        )
        .is_ok()
    );

    let values = vec![None, Some(9_u64)];
    let mut encoded = Vec::new();
    midnight_serialize::tagged_serialize(&values, &mut encoded).unwrap();
    assert!(
        run_command(
            &handle,
            serde_json::json!({
                "kind": "parseCheckResult",
                "resultBase64": encode_base64(&encoded)
            }),
        )
        .is_ok()
    );
    for command in [
        serde_json::json!({"kind": "createCheckPayload", "preimageBase64": ""}),
        serde_json::json!({"kind": "createProvingPayload", "preimageBase64": ""}),
        serde_json::json!({
            "kind": "canonicalizeTransaction",
            "signatureMarker": "bad",
            "proofMarker": "bad",
            "bindingMarker": "bad",
            "rawBase64": ""
        }),
    ] {
        assert!(run_command(&handle, command).is_err());
    }
}

#[test]
fn resumable_operation_variants_clear_or_preserve_state_on_failure() {
    let _runtime = isolated_runtime();
    let handle = open_test_session("coverage-resume", None);
    let session = session_for_handle(handle.id, handle.generation).unwrap();
    let wallet_state = lock_session(&session).unwrap().wallet_state.clone();
    let request = || transaction::RemoteProofRequest {
        key: "request".to_owned(),
        kind: transaction::RemoteProofKind::Check,
        body: vec![1],
    };
    let kinds = vec![
        PendingOperationKind::FinalizeTransactionProof {
            raw: vec![0xff],
            key_material: transaction::RemoteProofKeyMaterials::default(),
            proposed_state: Some(wallet_state.clone()),
            expected_identifiers: Vec::new(),
            responses: transaction::RemoteProofResponses::default(),
            pending_request: request(),
        },
        PendingOperationKind::FinalizeTransactionBalance {
            proposed_state: Some(wallet_state.clone()),
            expected_identifiers: Vec::new(),
        },
        PendingOperationKind::DappIntentProof {
            raw: vec![0xff],
            responses: transaction::RemoteProofResponses::default(),
            pending_request: request(),
        },
        PendingOperationKind::GenerateDustProof {
            raw: vec![0xff],
            proposed_state: wallet_state,
            expected_identifiers: Vec::new(),
            responses: transaction::RemoteProofResponses::default(),
            pending_request: request(),
        },
        PendingOperationKind::Balance {
            original_raw: vec![0xff],
            original_sealed: false,
            balancing_raw: vec![0xff],
            responses: transaction::RemoteProofResponses::default(),
            pending_request: request(),
        },
    ];
    for kind in kinds {
        let (operation, effect_id) =
            register_operation(handle.id, handle.generation, &session, kind).unwrap();
        assert!(resume_result(&operation, &effect_id, "rejected").is_err());
        assert!(lock_session(&session).unwrap().active_operation.is_none());
    }

    let (operation, effect_id, _) = install_submission_operation(&handle, 20);
    assert!(resume_result(&operation, &effect_id, "unexpected").is_err());
    assert_eq!(
        lock_session(&session).unwrap().active_operation,
        Some(operation.id)
    );
    cancel_operation(operation.id, operation.generation).unwrap();
}
