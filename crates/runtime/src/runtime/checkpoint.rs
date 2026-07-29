fn checkpoint_checksum(checkpoint: &WalletCheckpoint) -> String {
    let mut hasher = Sha256::new();
    hasher.update(checkpoint.version.to_le_bytes());
    update_len_prefixed(&mut hasher, checkpoint.network_id.as_bytes());
    update_len_prefixed(&mut hasher, checkpoint.wallet_fingerprint.as_bytes());
    update_len_prefixed(
        &mut hasher,
        checkpoint
            .legacy_state
            .serialized_dust_wallet_state
            .as_bytes(),
    );
    update_len_prefixed(
        &mut hasher,
        checkpoint
            .legacy_state
            .serialized_shielded_wallet_state
            .as_bytes(),
    );
    update_len_prefixed(
        &mut hasher,
        checkpoint
            .legacy_state
            .serialized_unshielded_wallet_state
            .as_bytes(),
    );
    let mut offsets = checkpoint.stream_offsets.clone();
    offsets.sort_by(|left, right| left.stream.cmp(&right.stream));
    hasher.update((offsets.len() as u64).to_le_bytes());
    for offset in offsets {
        update_len_prefixed(&mut hasher, offset.stream.as_bytes());
        hasher.update(offset.next_offset.to_le_bytes());
    }
    if !checkpoint.caught_up_streams.is_empty() {
        update_len_prefixed(&mut hasher, b"caught-up-streams-v1");
        let mut caught_up_streams = checkpoint.caught_up_streams.clone();
        caught_up_streams.sort();
        hasher.update((caught_up_streams.len() as u64).to_le_bytes());
        for stream in caught_up_streams {
            update_len_prefixed(&mut hasher, stream.as_bytes());
        }
    }
    let mut receipts = checkpoint.batch_receipts.clone();
    receipts.sort_by(|left, right| {
        (&left.stream, left.from_offset, left.to_offset).cmp(&(
            &right.stream,
            right.from_offset,
            right.to_offset,
        ))
    });
    hasher.update((receipts.len() as u64).to_le_bytes());
    for receipt in receipts {
        update_len_prefixed(&mut hasher, receipt.stream.as_bytes());
        hasher.update(receipt.from_offset.to_le_bytes());
        hasher.update(receipt.to_offset.to_le_bytes());
        update_len_prefixed(&mut hasher, receipt.digest.as_bytes());
    }
    if !checkpoint.pending_submissions.is_empty() {
        update_len_prefixed(&mut hasher, b"pending-submissions-v1");
        let mut submissions = checkpoint.pending_submissions.clone();
        submissions.sort_by(|left, right| left.transaction_hash.cmp(&right.transaction_hash));
        hasher.update((submissions.len() as u64).to_le_bytes());
        for submission in submissions {
            update_len_prefixed(&mut hasher, submission.transaction_hash.as_bytes());
            hasher.update((submission.identifiers.len() as u64).to_le_bytes());
            for identifier in submission.identifiers {
                update_len_prefixed(&mut hasher, identifier.as_bytes());
            }
            hasher.update(match submission.status {
                SubmissionStatus::AwaitingResponse => [0],
                SubmissionStatus::Accepted => [1],
                SubmissionStatus::Rejected => [2],
                SubmissionStatus::StatusUnknown => [3],
            });
        }
    }
    update_len_prefixed(&mut hasher, checkpoint.ledger_revision.as_bytes());
    hasher.update(checkpoint.generation.to_le_bytes());
    hex::encode(hasher.finalize())
}

fn encode_checkpoint(checkpoint: &WalletCheckpoint) -> Result<Vec<u8>, MidnightRuntimeError> {
    let payload =
        serde_json::to_vec(checkpoint).map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if payload.len() > MAX_CHECKPOINT_BYTES.saturating_sub(8) {
        return Err(MidnightRuntimeError::NativeInternal);
    }
    let mut encoded = Vec::with_capacity(8 + payload.len());
    encoded.extend_from_slice(CHECKPOINT_MAGIC);
    encoded.extend_from_slice(&CHECKPOINT_VERSION.to_be_bytes());
    encoded.extend_from_slice(&payload);
    Ok(encoded)
}

fn decode_checkpoint(raw: &[u8]) -> Result<WalletCheckpoint, MidnightRuntimeError> {
    if raw.len() < 8 || raw.len() > MAX_CHECKPOINT_BYTES || &raw[..4] != CHECKPOINT_MAGIC {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    let version = u32::from_be_bytes(
        raw[4..8]
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?,
    );
    if version != CHECKPOINT_VERSION {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    let checkpoint: WalletCheckpoint = serde_json::from_slice(&raw[8..])
        .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
    validate_checkpoint(checkpoint)
}

fn validate_checkpoint(
    checkpoint: WalletCheckpoint,
) -> Result<WalletCheckpoint, MidnightRuntimeError> {
    if checkpoint.version != CHECKPOINT_VERSION || checkpoint.ledger_revision != SOURCE_REVISION {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    if checkpoint.checksum.len() != 64 || checkpoint.checksum != checkpoint_checksum(&checkpoint) {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    let mut streams = HashSet::new();
    for offset in &checkpoint.stream_offsets {
        if !valid_checkpoint_stream(&offset.stream) || !streams.insert(offset.stream.as_str()) {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
    }
    let mut caught_up_streams = HashSet::new();
    for stream in &checkpoint.caught_up_streams {
        if !valid_checkpoint_stream(stream) || !caught_up_streams.insert(stream.as_str()) {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
    }
    if checkpoint.batch_receipts.len() > MAX_SYNC_BATCH_RECEIPTS {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    let mut receipt_keys = HashSet::new();
    for receipt in &checkpoint.batch_receipts {
        let receipt_key = BatchKey {
            stream: receipt.stream.clone(),
            from_offset: receipt.from_offset,
            to_offset: receipt.to_offset,
        };
        if !valid_checkpoint_stream(&receipt.stream)
            || receipt.to_offset < receipt.from_offset
            || receipt.digest.len() != 64
            || !receipt
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            || !receipt_keys.insert(receipt_key)
        {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
        let next_offset = checkpoint
            .stream_offsets
            .iter()
            .find(|offset| offset.stream == receipt.stream)
            .map(|offset| offset.next_offset)
            .ok_or(MidnightRuntimeError::StateIncompatible)?;
        if receipt.to_offset > next_offset {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
    }
    let mut transaction_hashes = HashSet::new();
    for submission in &checkpoint.pending_submissions {
        if !valid_lower_hex(&submission.transaction_hash, 64)
            || !transaction_hashes.insert(submission.transaction_hash.as_str())
            || submission
                .identifiers
                .iter()
                .any(|identifier| !valid_variable_lower_hex(identifier))
        {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
    }
    Ok(checkpoint)
}

fn valid_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_variable_lower_hex(value: &str) -> bool {
    !value.is_empty() && value.len().is_multiple_of(2) && valid_lower_hex(value, value.len())
}

fn payload_digest(payloads: &[Vec<u8>]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((payloads.len() as u64).to_le_bytes());
    for payload in payloads {
        update_len_prefixed(&mut hasher, payload);
    }
    hex::encode(hasher.finalize())
}

fn prune_batch_receipts(receipts: &mut HashMap<BatchKey, String>) {
    while receipts.len() > MAX_SYNC_BATCH_RECEIPTS {
        let oldest = receipts
            .keys()
            .min_by(|left, right| {
                (left.to_offset, &left.stream, left.from_offset).cmp(&(
                    right.to_offset,
                    &right.stream,
                    right.from_offset,
                ))
            })
            .cloned();
        if let Some(oldest) = oldest {
            receipts.remove(&oldest);
        } else {
            break;
        }
    }
}

fn snapshot(state: &SessionState) -> Result<WalletSnapshot, MidnightRuntimeError> {
    let synced = ["shielded", "unshielded", "dust"]
        .iter()
        .all(|stream| state.caught_up_streams.contains(*stream));
    Ok(WalletSnapshot {
        wallet_fingerprint: state.config.wallet_fingerprint.clone(),
        network_id: state.config.network_id.clone(),
        status: if synced { "ready" } else { "syncing" },
        generation: state.generation,
        stream_offsets: sorted_offsets(&state.offsets),
        unshielded_address: state.config.unshielded_address.clone(),
        shielded_coin_public_key_hex: state.address_material.shielded_coin_public_key_hex.clone(),
        shielded_encryption_public_key_hex: state
            .address_material
            .shielded_encryption_public_key_hex
            .clone(),
        dust_public_key: state.address_material.dust_public_key.clone(),
        balances: state.wallet_state.balances()?,
        pending_submissions: sorted_pending_submissions(&state.pending_submissions),
    })
}
