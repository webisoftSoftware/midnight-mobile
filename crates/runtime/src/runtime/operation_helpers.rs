fn parse_dapp_inputs(
    values: Vec<RuntimeDappInput>,
) -> Result<Vec<DappIntentInput>, MidnightRuntimeError> {
    values
        .into_iter()
        .map(|value| {
            Ok(DappIntentInput {
                wallet_type: value.wallet_type,
                token_type: value.token_type,
                amount: value
                    .amount
                    .parse::<u128>()
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
            })
        })
        .collect()
}

fn parse_dapp_outputs(
    values: Vec<RuntimeDappOutput>,
) -> Result<Vec<DappTransactionOutput>, MidnightRuntimeError> {
    values
        .into_iter()
        .map(|value| {
            Ok(DappTransactionOutput {
                wallet_type: value.wallet_type,
                token_type: value.token_type,
                amount: value
                    .amount
                    .parse::<u128>()
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
                receiver_address: value.receiver_address,
            })
        })
        .collect()
}

fn proof_effect(kind: transaction::RemoteProofKind) -> &'static str {
    match kind {
        transaction::RemoteProofKind::Check => "check",
        transaction::RemoteProofKind::Prove => "prove",
    }
}

fn take_proving_key_material(
    key_material: &mut Option<RuntimeProvingKeyMaterial>,
) -> Result<Option<ProvingKeyMaterial>, MidnightRuntimeError> {
    let Some(mut material) = key_material.take() else {
        return Ok(None);
    };
    let compression = material.compression.take();
    let decode = |value: &mut String| {
        let decoded = decode_base64(value);
        value.zeroize();
        let decoded = Zeroizing::new(decoded?);
        match compression.as_deref() {
            None => Ok(decoded),
            Some("gzip") => Ok(Zeroizing::new(codec::decompress_gzip(&decoded)?)),
            Some(_) => Err(MidnightRuntimeError::InvalidArgument),
        }
    };
    let prover_key = decode(&mut material.prover_key_base64)?;
    let verifier_key = decode(&mut material.verifier_key_base64)?;
    let ir_source = decode(&mut material.ir_base64)?;
    Ok(Some(ProvingKeyMaterial {
        prover_key: prover_key.to_vec(),
        verifier_key: verifier_key.to_vec(),
        ir_source: ir_source.to_vec(),
    }))
}

fn take_proving_key_material_map(
    key_material: &mut Option<BTreeMap<String, RuntimeProvingKeyMaterial>>,
) -> Result<transaction::RemoteProofKeyMaterials, MidnightRuntimeError> {
    let Some(materials) = key_material.take() else {
        return Ok(transaction::RemoteProofKeyMaterials::default());
    };
    if materials.is_empty() {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut decoded = transaction::RemoteProofKeyMaterials::default();
    for (location, material) in materials {
        if location.is_empty() || location.len() > 1024 {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let mut material = Some(material);
        let material = take_proving_key_material(&mut material)?
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        decoded.insert(location, material)?;
    }
    Ok(decoded)
}

fn next_effect_id(generation: u64, operation_id: u64, current: &str) -> String {
    let sequence = current
        .rsplit(':')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_add(1);
    format!("{generation}:{operation_id}:{sequence}")
}

fn register_operation(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    kind: PendingOperationKind,
) -> Result<(OperationHandle, String), MidnightRuntimeError> {
    register_operation_with_reservation(session_id, generation, session, kind, false)
}

fn register_reserved_operation(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    kind: PendingOperationKind,
) -> Result<(OperationHandle, String), MidnightRuntimeError> {
    register_operation_with_reservation(session_id, generation, session, kind, true)
}

fn register_operation_with_reservation(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    kind: PendingOperationKind,
    reserved: bool,
) -> Result<(OperationHandle, String), MidnightRuntimeError> {
    let mut runtime = lock_registry()?;
    let registered_session = runtime
        .sessions
        .get(&session_id)
        .filter(|registered| Arc::ptr_eq(registered, session));
    if registered_session.is_none() {
        return Err(MidnightRuntimeError::StaleSession);
    }
    if runtime
        .operations
        .values()
        .any(|operation| operation.session_id == session_id)
    {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let mut session_state = lock_session(session)?;
    if session_state.generation != generation || session_state.closing {
        return Err(MidnightRuntimeError::StaleSession);
    }
    let expected_active = reserved.then_some(RESERVED_OPERATION_ID);
    if session_state.active_operation != expected_active {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let operation_id = runtime.next_id;
    let next_id = operation_id
        .checked_add(1)
        .ok_or(MidnightRuntimeError::Unavailable)?;
    runtime.next_id = next_id;
    let effect_id = format!("{generation}:{operation_id}:1");
    runtime.operations.insert(
        operation_id,
        PendingOperation {
            generation,
            session_id,
            effect_id: effect_id.clone(),
            kind,
        },
    );
    session_state.active_operation = Some(operation_id);
    Ok((
        OperationHandle {
            id: operation_id,
            generation,
        },
        effect_id,
    ))
}

fn reserve_operation(state: &mut SessionState) -> Result<(), MidnightRuntimeError> {
    if state.active_operation.is_some() {
        return Err(MidnightRuntimeError::Unavailable);
    }
    state.active_operation = Some(RESERVED_OPERATION_ID);
    Ok(())
}

fn clear_operation_reservation(session: &Arc<Mutex<SessionState>>) {
    if let Ok(mut state) = session.lock()
        && state.active_operation == Some(RESERVED_OPERATION_ID)
    {
        state.active_operation = None;
    }
}

struct TransactionFinalizationInput {
    network_id: String,
    raw: Vec<u8>,
    key_material: transaction::RemoteProofKeyMaterials,
    proposed_state: Option<NativeWalletState>,
    expected_identifiers: Vec<String>,
}

fn start_transaction_finalization(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    input: TransactionFinalizationInput,
) -> Result<String, MidnightRuntimeError> {
    let mut registered_operation = None;
    let result = (|| {
        let responses = transaction::RemoteProofResponses::default();
        let progress = transaction::advance_unproven_transaction_with_materials(
            &input.raw,
            &input.network_id,
            &responses,
            &input.key_material,
        )?;
        let TransactionFinalizationInput {
            raw,
            key_material,
            proposed_state,
            expected_identifiers,
            ..
        } = input;
        match progress {
            transaction::BalanceProgress::Network(pending_request) => {
                let body = Zeroizing::new(pending_request.body.clone());
                let effect = proof_effect(pending_request.kind);
                let (operation, effect_id) = register_reserved_operation(
                    session_id,
                    generation,
                    session,
                    PendingOperationKind::FinalizeTransactionProof {
                        raw,
                        key_material,
                        proposed_state,
                        expected_identifiers,
                        responses,
                        pending_request,
                    },
                )?;
                registered_operation = Some(operation.id);
                to_json(&OperationStep {
                    kind: "network",
                    operation: Some(operation),
                    effect_id: Some(effect_id),
                    effect: Some(effect),
                    endpoint_role: Some("proof"),
                    body_base64: Some(encode_base64(&body)),
                    result_json: None,
                })
            }
            transaction::BalanceProgress::Complete(finalized) => start_transaction_balance(
                session_id,
                generation,
                session,
                proposed_state,
                expected_identifiers,
                finalized,
                &mut registered_operation,
            ),
        }
    })();
    if result.is_err() {
        if let Some(operation_id) = registered_operation {
            discard_operation(operation_id);
        } else {
            clear_operation_reservation(session);
        }
    }
    result
}

fn start_transaction_balance(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    proposed_state: Option<NativeWalletState>,
    expected_identifiers: Vec<String>,
    finalized: transaction::FinalizedTransaction,
    registered_operation: &mut Option<u64>,
) -> Result<String, MidnightRuntimeError> {
    if expected_identifiers
        .iter()
        .any(|identifier| !finalized.identifiers.contains(identifier))
    {
        return Err(MidnightRuntimeError::ProofFailed);
    }
    let body = finalized.canonical;
    let (operation, effect_id) = register_reserved_operation(
        session_id,
        generation,
        session,
        PendingOperationKind::FinalizeTransactionBalance {
            proposed_state,
            expected_identifiers,
        },
    )?;
    *registered_operation = Some(operation.id);
    to_json(&OperationStep {
        kind: "network",
        operation: Some(operation),
        effect_id: Some(effect_id),
        effect: Some("balance"),
        endpoint_role: Some("proof"),
        body_base64: Some(encode_base64(&body)),
        result_json: None,
    })
}

fn discard_operation(operation_id: u64) {
    if let Ok(mut runtime) = lock_registry()
        && let Some(operation) = runtime.operations.remove(&operation_id)
        && let Some(session) = runtime.sessions.get(&operation.session_id)
        && let Ok(mut state) = session.lock()
        && state.active_operation == Some(operation_id)
    {
        state.active_operation = None;
    }
}

fn clear_active_operation(state: &mut SessionState, operation_id: u64) {
    if state.active_operation == Some(operation_id) {
        state.active_operation = None;
    }
}

fn clear_orphaned_active_operation(session: &Arc<Mutex<SessionState>>, operation_id: u64) {
    let operation_is_registered = lock_registry()
        .map(|runtime| runtime.operations.contains_key(&operation_id))
        .unwrap_or(true);
    if operation_is_registered {
        return;
    }
    if let Ok(mut state) = session.lock() {
        clear_active_operation(&mut state, operation_id);
    }
}

fn normalize_transaction_hash(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    let normalized = normalized
        .strip_prefix("0x")
        .or_else(|| normalized.strip_prefix("shielded-"))
        .unwrap_or(&normalized);
    valid_lower_hex(normalized, 64).then(|| normalized.to_owned())
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumericValue {
        Integer(u64),
        Decimal(String),
    }

    match Option::<NumericValue>::deserialize(deserializer)? {
        None => Ok(None),
        Some(NumericValue::Integer(value)) => Ok(Some(value)),
        Some(NumericValue::Decimal(value)) => value
            .parse::<u64>()
            .ok()
            .filter(|parsed| parsed.to_string() == value)
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("expiresAt must be a canonical u64")),
    }
}

fn decode_balance_service_result(
    result: &NetworkResult,
    expected_network_id: &str,
    expected_identifiers: &[String],
) -> Result<
    (
        BalanceServiceResult,
        transaction::FinalizedTransaction,
        String,
    ),
    MidnightRuntimeError,
> {
    let body = result
        .body_base64
        .as_deref()
        .ok_or(MidnightRuntimeError::ProofFailed)
        .and_then(decode_base64)?;
    let response: BalanceServiceResult =
        serde_json::from_slice(&body).map_err(|_| MidnightRuntimeError::ProofFailed)?;
    let raw = hex::decode(&response.tx_bytes).map_err(|_| MidnightRuntimeError::ProofFailed)?;
    let finalized = transaction::validate_finalized_transaction(&raw, expected_network_id)
        .map_err(|_| MidnightRuntimeError::ProofFailed)?;
    if expected_identifiers
        .iter()
        .any(|identifier| !finalized.identifiers.contains(identifier))
    {
        return Err(MidnightRuntimeError::ProofFailed);
    }
    let actual_hash = hex::encode(Sha256::digest(&raw));
    if normalize_transaction_hash(&response.tx_hash).as_deref() != Some(actual_hash.as_str()) {
        return Err(MidnightRuntimeError::ProofFailed);
    }
    Ok((response, finalized, actual_hash))
}
