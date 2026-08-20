use super::*;

pub(super) fn open_wallet_session(
    config_json: String,
    mut night_external_key: Vec<u8>,
    mut zswap_seed: Vec<u8>,
    mut dust_seed: Vec<u8>,
    mut checkpoint: Option<Vec<u8>>,
) -> Result<RuntimeSessionHandle, MidnightRuntimeError> {
    let result = (|| {
        let config = validate_config(
            serde_json::from_str(&config_json)
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
        )?;
        let address_material =
            derive_wallet_address_material_inner(&night_external_key, &zswap_seed, &dust_seed)?;
        let checkpoint = checkpoint.as_deref().map(decode_checkpoint).transpose()?;
        if checkpoint.as_ref().is_some_and(|checkpoint| {
            checkpoint.network_id != config.network_id
                || checkpoint.wallet_fingerprint != config.wallet_fingerprint
        }) {
            return Err(MidnightRuntimeError::StateIncompatible);
        }

        let legacy_state = checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.legacy_state.clone())
            .unwrap_or_default();
        let wallet_state = NativeWalletState::restore(
            &legacy_state,
            RestoreContext {
                network_id: &config.network_id,
                unshielded_address: &config.unshielded_address,
                address_material: &address_material,
                night_external_key: &night_external_key,
            },
        )?;

        let mut runtime = lock_registry()?;
        if runtime.sessions.len() >= MAX_OPEN_SESSIONS {
            return Err(MidnightRuntimeError::Unavailable);
        }
        if let Some(checkpoint) = &checkpoint {
            let checkpoint_successor = checkpoint
                .generation
                .checked_add(1)
                .ok_or(MidnightRuntimeError::StateIncompatible)?;
            runtime.next_generation = runtime.next_generation.max(checkpoint_successor);
        }
        let id = runtime.next_id;
        let generation = runtime.next_generation;
        let next_id = id.checked_add(1).ok_or(MidnightRuntimeError::Unavailable)?;
        let next_generation = generation
            .checked_add(1)
            .ok_or(MidnightRuntimeError::Unavailable)?;
        runtime.next_id = next_id;
        runtime.next_generation = next_generation;

        let mut offsets = HashMap::new();
        let mut applied_batches = HashMap::new();
        let mut seen_streams = HashSet::new();
        let mut caught_up_streams = HashSet::new();
        let mut pending_submissions = HashMap::new();
        if let Some(checkpoint) = checkpoint {
            for offset in checkpoint.stream_offsets {
                seen_streams.insert(offset.stream.clone());
                offsets.insert(offset.stream, offset.next_offset);
            }
            for receipt in checkpoint.batch_receipts {
                applied_batches.insert(
                    BatchKey {
                        stream: receipt.stream,
                        from_offset: receipt.from_offset,
                        to_offset: receipt.to_offset,
                    },
                    receipt.digest,
                );
            }
            for stream in checkpoint.caught_up_streams {
                caught_up_streams.insert(stream);
            }
            for submission in checkpoint.pending_submissions {
                pending_submissions.insert(submission.transaction_hash.clone(), submission);
            }
        }
        let secrets = SessionSecrets {
            night_external_key: std::mem::take(&mut night_external_key),
            zswap_seed: std::mem::take(&mut zswap_seed),
            dust_seed: std::mem::take(&mut dust_seed),
        };
        runtime.sessions.insert(
            id,
            Arc::new(Mutex::new(SessionState {
                generation,
                config,
                secrets,
                address_material,
                wallet_state,
                offsets,
                applied_batches,
                seen_streams,
                caught_up_streams,
                pending_submissions,
                active_operation: None,
                closing: false,
            })),
        );
        Ok(RuntimeSessionHandle { id, generation })
    })();

    // Buffers moved into SessionSecrets are empty here. On every error path,
    // the original FFI-owned bytes are still present and are cleared here.
    night_external_key.zeroize();
    zswap_seed.zeroize();
    dust_seed.zeroize();
    if let Some(checkpoint_bytes) = checkpoint.as_mut() {
        checkpoint_bytes.zeroize();
    }
    result
}

pub(super) fn apply_sync_batch(
    session_id: u64,
    generation: u64,
    stream: String,
    from_offset: u64,
    to_offset: u64,
    payloads: Vec<Vec<u8>>,
) -> Result<String, MidnightRuntimeError> {
    let canonical_stream =
        canonical_stream(&stream).ok_or(MidnightRuntimeError::InvalidArgument)?;
    if to_offset < from_offset {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let total_bytes = payloads.iter().try_fold(0_usize, |total, payload| {
        total
            .checked_add(payload.len())
            .ok_or(MidnightRuntimeError::InvalidArgument)
    })?;
    if total_bytes > MAX_SYNC_PAYLOAD_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let digest = payload_digest(&payloads);
    let key = BatchKey {
        stream: canonical_stream.to_owned(),
        from_offset,
        to_offset,
    };
    let session = session_for_handle(session_id, generation)?;
    let mut state = lock_session(&session)?;
    if state.closing || state.generation != generation {
        return Err(MidnightRuntimeError::StaleSession);
    }
    if state.active_operation.is_some() {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let expected = state.offsets.get(canonical_stream).copied().unwrap_or(0);
    if is_tip_marker(&stream) {
        if !payloads.is_empty() || from_offset != expected || to_offset != expected {
            return Err(MidnightRuntimeError::SyncGap);
        }
        state.caught_up_streams.insert(canonical_stream.to_owned());
        return to_json(&ApplySyncResult {
            duplicate: false,
            snapshot: snapshot(&state)?,
        });
    }
    if from_offset < expected {
        return match state.applied_batches.get(&key) {
            Some(previous) if previous == &digest => to_json(&ApplySyncResult {
                duplicate: true,
                snapshot: snapshot(&state)?,
            }),
            _ => Err(MidnightRuntimeError::SyncGap),
        };
    }
    if from_offset != expected {
        return Err(MidnightRuntimeError::SyncGap);
    }
    if let Some(previous) = state.applied_batches.get(&key) {
        if previous == &digest {
            return to_json(&ApplySyncResult {
                duplicate: true,
                snapshot: snapshot(&state)?,
            });
        }
        return Err(MidnightRuntimeError::SyncGap);
    }

    NativeWalletState::validate_wire_offsets(&stream, &payloads, from_offset, to_offset)?;
    NativeWalletState::validate_v2_trailer(&stream, &payloads, to_offset)?;

    // Decode and apply against cloned native wallet state. None of the stream
    // offsets, receipts, or wallet structures are committed until the entire
    // batch succeeds.
    let proposed_wallet_state = state.wallet_state.apply_batch(
        &stream,
        &payloads,
        &state.secrets.zswap_seed,
        &state.secrets.dust_seed,
    )?;
    state.wallet_state = proposed_wallet_state;
    state.offsets.insert(canonical_stream.to_owned(), to_offset);
    state.caught_up_streams.remove(canonical_stream);
    state.applied_batches.insert(key, digest);
    prune_batch_receipts(&mut state.applied_batches);
    state.seen_streams.insert(canonical_stream.to_owned());
    to_json(&ApplySyncResult {
        duplicate: false,
        snapshot: snapshot(&state)?,
    })
}

pub(super) fn get_wallet_snapshot(
    session_id: u64,
    generation: u64,
) -> Result<String, MidnightRuntimeError> {
    let session = session_for_handle(session_id, generation)?;
    let state = lock_session(&session)?;
    to_json(&snapshot(&state)?)
}

pub(super) fn export_wallet_checkpoint(
    session_id: u64,
    generation: u64,
) -> Result<Vec<u8>, MidnightRuntimeError> {
    let session = session_for_handle(session_id, generation)?;
    let state = lock_session(&session)?;
    let legacy_state = state.wallet_state.export_legacy(LegacyExportContext {
        network_id: &state.config.network_id,
        unshielded_address: &state.config.unshielded_address,
        address_material: &state.address_material,
        night_external_key: &state.secrets.night_external_key,
        shielded_offset: state.offsets.get("shielded").copied(),
        dust_offset: state.offsets.get("dust").copied(),
        unshielded_offset: state.offsets.get("unshielded").copied(),
    })?;
    let mut checkpoint = WalletCheckpoint {
        version: CHECKPOINT_VERSION,
        network_id: state.config.network_id.clone(),
        wallet_fingerprint: state.config.wallet_fingerprint.clone(),
        legacy_state,
        stream_offsets: sorted_offsets(&state.offsets),
        caught_up_streams: {
            let mut streams = state.caught_up_streams.iter().cloned().collect::<Vec<_>>();
            streams.sort();
            streams
        },
        batch_receipts: sorted_receipts(&state.applied_batches),
        pending_submissions: sorted_pending_submissions(&state.pending_submissions),
        ledger_revision: SOURCE_REVISION.to_owned(),
        generation: state.generation,
        checksum: String::new(),
    };
    checkpoint.checksum = checkpoint_checksum(&checkpoint);
    encode_checkpoint(&checkpoint)
}
