#[test]
fn command_decoder_accepts_exactly_the_wallet_core_union() {
    let accepted = [
        r#"{"kind":"signData","domain":"example.test","dataBase64":""}"#,
        r#"{"kind":"createCheckPayload","preimageBase64":""}"#,
        r#"{"kind":"parseCheckResult","resultBase64":""}"#,
        r#"{"kind":"createProvingPayload","preimageBase64":""}"#,
        r#"{"kind":"canonicalizeTransaction","signatureMarker":"a","proofMarker":"b","bindingMarker":"c","rawBase64":""}"#,
        r#"{"kind":"createSyncRequest","stream":"shielded","fromOffset":0,"limit":100}"#,
        r#"{"kind":"createShieldedSpentRequest"}"#,
        r#"{"kind":"applyShieldedSpentResponse","resultBase64":""}"#,
        r#"{"kind":"setShieldedProtocolVersion","protocolVersion":1,"syncOffset":0}"#,
        r#"{"kind":"createDustSpendRequest"}"#,
        r#"{"kind":"createDustCommitmentRequest","syncOffset":0,"rawBase64":""}"#,
        r#"{"kind":"applyDustSpendResolution","syncOffset":0,"rawBase64":"","resultBase64":""}"#,
        r#"{"kind":"transfer","to":"a","amount":"1","tokenType":"b","walletType":"shielded"}"#,
        r#"{"kind":"dappTransfer","outputs":[]}"#,
        r#"{"kind":"dappIntent","inputs":[],"outputs":[]}"#,
        r#"{"kind":"generateDust","ledgerParametersBase64":"","feeBlocksMargin":0,"additionalFeeOverhead":"0"}"#,
        r#"{"kind":"balanceUnsealed","rawBase64":"","ledgerParametersBase64":"","feeBlocksMargin":0,"additionalFeeOverhead":"0"}"#,
        r#"{"kind":"balanceSealed","rawBase64":"","ledgerParametersBase64":"","feeBlocksMargin":0,"additionalFeeOverhead":"0"}"#,
        r#"{"kind":"submitFinalized","rawBase64":""}"#,
    ];
    assert_eq!(accepted.len(), 19);
    for command in accepted {
        serde_json::from_str::<RuntimeCommand>(command).unwrap();
    }
}

#[test]
fn command_decoder_rejects_removed_unknown_and_extra_fields() {
    for command in [
        r#"{"kind":"removedCommand","dataBase64":""}"#,
        r#"{"kind":"createSyncRequest","stream":"shielded","fromOffset":0,"limit":100,"path":"/private"}"#,
        r#"{"kind":"notACommand"}"#,
    ] {
        assert!(serde_json::from_str::<RuntimeCommand>(command).is_err());
    }
}

#[test]
fn operation_step_embeds_results_as_objects_for_native_decoders() {
    let serialized = complete_json(
        r#"{"transactionBase64":"","transactionHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ledgerTransactionHash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","identifiers":[]}"#
            .to_owned(),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert!(value["result"].is_object());
    assert!(value.get("resultJson").is_none());
    assert!(value["operation"].is_null());
}

#[test]
fn snapshot_and_command_json_match_the_typescript_contract() {
    let _runtime = isolated_runtime();
    let command: RuntimeCommand = serde_json::from_str(
        r#"{"kind":"createSyncRequest","stream":"dust","fromOffset":7,"limit":20}"#,
    )
    .unwrap();
    assert!(matches!(
        command,
        RuntimeCommand::CreateSyncRequest {
            stream,
            from_offset: 7,
            limit: 20,
        } if stream == "dust"
    ));
    let handle = open_wallet_session(
        r#"{"networkId":"preview","walletFingerprint":"wallet","unshieldedAddress":"address"}"#
            .to_owned(),
        vec![1; 32],
        vec![2; 32],
        vec![3; 32],
        None,
    )
    .unwrap();
    let snapshot_fields: serde_json::Value =
        serde_json::from_str(&get_wallet_snapshot(handle.id, handle.generation).unwrap()).unwrap();
    assert_eq!(snapshot_fields["walletFingerprint"], "wallet");
    assert!(snapshot_fields.get("fingerprint").is_none());
    close_wallet_session(handle.id, handle.generation).unwrap();
}

#[test]
fn sign_data_result_matches_the_typescript_hex_contract() {
    let _runtime = isolated_runtime();
    let handle = open_wallet_session(
        r#"{"networkId":"preview","walletFingerprint":"signer","unshieldedAddress":"address"}"#
            .to_owned(),
        vec![1; 32],
        vec![2; 32],
        vec![3; 32],
        None,
    )
    .unwrap();
    let raw = begin_command(
        handle.id,
        handle.generation,
        r#"{"kind":"signData","domain":"example.test","dataBase64":"AA=="}"#.to_owned(),
    )
    .unwrap();
    let step: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(step["result"]["signatureHex"].as_str().unwrap().len(), 128);
    assert_eq!(
        step["result"]["verifyingKeyHex"].as_str().unwrap().len(),
        64
    );
    close_wallet_session(handle.id, handle.generation).unwrap();
}

#[test]
fn balance_service_expiry_accepts_wire_strings_and_serializes_as_a_number() {
    let decoded: BalanceServiceResult = serde_json::from_str(
        r#"{"txHash":"aa","txBytes":"bb","expiresAt":"18446744073709551615"}"#,
    )
    .unwrap();
    assert_eq!(decoded.expires_at, Some(u64::MAX));
}

#[test]
fn signing_transcript_is_versioned_length_prefixed_and_domain_separated() {
    let first = signing_transcript("example.one", b"same-data").unwrap();
    let second = signing_transcript("example.two", b"same-data").unwrap();
    assert_ne!(first, second);
    assert!(first.starts_with(b"midnight-mobile/sign-data\x01"));
    assert!(signing_transcript("", b"data").is_err());
}

#[test]
fn checkpoint_envelope_rejects_plaintext_json_and_unknown_versions() {
    assert!(decode_checkpoint(br#"{"version":1}"#).is_err());
    let mut unknown = Vec::from(CHECKPOINT_MAGIC.as_slice());
    unknown.extend_from_slice(&2_u32.to_be_bytes());
    unknown.extend_from_slice(b"{}");
    assert!(decode_checkpoint(&unknown).is_err());
}
