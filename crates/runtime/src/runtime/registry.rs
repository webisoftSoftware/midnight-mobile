static REGISTRY: OnceLock<Mutex<RuntimeRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<RuntimeRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(RuntimeRegistry::default()))
}

fn lock_registry() -> Result<MutexGuard<'static, RuntimeRegistry>, MidnightRuntimeError> {
    registry()
        .lock()
        .map_err(|_| MidnightRuntimeError::NativeInternal)
}

fn lock_session<'a>(
    session: &'a Arc<Mutex<SessionState>>,
) -> Result<MutexGuard<'a, SessionState>, MidnightRuntimeError> {
    session.lock().map_err(|_| MidnightRuntimeError::NativeInternal)
}

fn canonical_stream(stream: &str) -> Option<&'static str> {
    match stream {
        "shielded" | "shielded-wire" | "shielded-v2" | "shielded-tip" => Some("shielded"),
        "unshielded" | "unshielded-tip" => Some("unshielded"),
        "dust" | "dust-wire" | "dust-v2" | "dust-tip" => Some("dust"),
        _ => None,
    }
}

fn is_tip_marker(stream: &str) -> bool {
    matches!(stream, "shielded-tip" | "unshielded-tip" | "dust-tip")
}

fn valid_checkpoint_stream(stream: &str) -> bool {
    matches!(stream, "shielded" | "unshielded" | "dust")
}

fn validate_config(config: WalletSessionConfig) -> Result<WalletSessionConfig, MidnightRuntimeError> {
    let valid_network = matches!(
        config.network_id.as_str(),
        "preview" | "preprod" | "mainnet"
    );
    let valid_fingerprint =
        !config.wallet_fingerprint.trim().is_empty() && config.wallet_fingerprint.len() <= 256;
    let valid_address =
        !config.unshielded_address.trim().is_empty() && config.unshielded_address.len() <= 256;
    if !valid_network || !valid_fingerprint || !valid_address {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok(config)
}

fn session_for_handle(
    session_id: u64,
    generation: u64,
) -> Result<Arc<Mutex<SessionState>>, MidnightRuntimeError> {
    let session = {
        let runtime = lock_registry()?;
        runtime.sessions.get(&session_id).cloned()
    }
    .ok_or(MidnightRuntimeError::StaleSession)?;
    {
        let state = lock_session(&session)?;
        if state.generation != generation || state.closing {
            return Err(MidnightRuntimeError::StaleSession);
        }
    }
    Ok(session)
}

fn sorted_offsets(offsets: &HashMap<String, u64>) -> Vec<StreamOffset> {
    let mut values = offsets
        .iter()
        .map(|(stream, next_offset)| StreamOffset {
            stream: stream.clone(),
            next_offset: *next_offset,
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.stream.cmp(&right.stream));
    values
}

fn sorted_receipts(receipts: &HashMap<BatchKey, String>) -> Vec<BatchReceipt> {
    let mut values = receipts
        .iter()
        .map(|(key, digest)| BatchReceipt {
            stream: key.stream.clone(),
            from_offset: key.from_offset,
            to_offset: key.to_offset,
            digest: digest.clone(),
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        (&left.stream, left.from_offset, left.to_offset).cmp(&(
            &right.stream,
            right.from_offset,
            right.to_offset,
        ))
    });
    values
}

fn sorted_pending_submissions(
    submissions: &HashMap<String, PendingSubmission>,
) -> Vec<PendingSubmission> {
    let mut values = submissions.values().cloned().collect::<Vec<_>>();
    values.sort_by(|left, right| left.transaction_hash.cmp(&right.transaction_hash));
    values
}

fn update_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}
