use super::super::*;

pub(super) fn handle(
    mut command: RuntimeCommand,
    mut state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    match &mut command {
        RuntimeCommand::SignData {
            domain,
            data_base64,
        } => {
            let data = decode_base64(data_base64)?;
            let result_json = sign_data_locked(&state, domain, &data)?;
            return complete_json(result_json);
        }
        RuntimeCommand::CreateSyncRequest {
            stream,
            from_offset,
            limit,
            mode,
        } => {
            let stream = match stream.as_str() {
                "shielded" => "shielded",
                "unshielded" => "unshielded",
                "dust" => "dust",
                _ => return Err(MidnightRuntimeError::InvalidArgument),
            };
            let next_offset = state.offsets.get(stream).copied().unwrap_or(0);
            if next_offset != *from_offset {
                return Err(MidnightRuntimeError::SyncGap);
            }
            let request = match mode {
                SyncRequestMode::Standard => {
                    let limit = limit
                        .as_ref()
                        .ok_or(MidnightRuntimeError::InvalidArgument)?;
                    if *limit == 0 || *limit > 4096 {
                        return Err(MidnightRuntimeError::InvalidArgument);
                    }
                    serde_json::to_vec(&serde_json::json!({
                        "stream": stream,
                        "fromOffset": from_offset,
                        "limit": limit,
                    }))
                    .map_err(|_| MidnightRuntimeError::NativeInternal)?
                }
                SyncRequestMode::Fast => {
                    if limit.is_some() || stream == "unshielded" {
                        return Err(MidnightRuntimeError::InvalidArgument);
                    }
                    NativeWalletState::snapshot_sync_request(
                        stream,
                        &state.secrets.zswap_seed,
                        &state.secrets.dust_seed,
                    )?
                }
            };
            return complete_json(to_json(&serde_json::json!({
                "stream": stream,
                "fromOffset": from_offset,
                "requestBase64": encode_base64(&request),
            }))?);
        }
        RuntimeCommand::DeriveShieldedMintContext => {
            if state.active_operation.is_some() {
                return Err(MidnightRuntimeError::Unavailable);
            }
            let context = state
                .wallet_state
                .shielded_mint_context(&state.secrets.zswap_seed)?;
            return complete_json(to_json(&serde_json::json!({
                "coinPublicKeyHex": serializable_hex(&context.coin_public_key)?,
                "encryptionPublicKeyHex": serializable_hex(&context.encryption_public_key)?,
                "outputIndex": context.output_index,
            }))?);
        }
        RuntimeCommand::WatchShieldedMint {
            coin_info_base64,
            expected_output_index,
        } => {
            if state.active_operation.is_some() {
                return Err(MidnightRuntimeError::Unavailable);
            }
            let raw_coin = decode_base64(coin_info_base64)?;
            let (proposed, output_index) = state.wallet_state.watch_shielded_mint(
                &raw_coin,
                *expected_output_index,
                &state.secrets.zswap_seed,
            )?;
            state.wallet_state = proposed;
            return complete_json(to_json(&serde_json::json!({
                "outputIndex": output_index,
            }))?);
        }
        RuntimeCommand::CreateShieldedSpentRequest => {
            let body = state.wallet_state.shielded_spent_request()?;
            let nullifier_count = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| value.get("nullifierPrefixes")?.as_array().map(Vec::len))
                .ok_or(MidnightRuntimeError::NativeInternal)?;
            return complete_json(to_json(&serde_json::json!({
                "requestBase64": encode_base64(&body),
                "nullifierCount": nullifier_count,
            }))?);
        }
        RuntimeCommand::ApplyShieldedSpentResponse { result_base64 } => {
            let raw = decode_base64(result_base64)?;
            let (proposed, removed_count) = state
                .wallet_state
                .apply_shielded_spent_response(&raw, &state.secrets.zswap_seed)?;
            state.wallet_state = proposed;
            return complete_json(to_json(&serde_json::json!({
                "removedCount": removed_count,
            }))?);
        }
        RuntimeCommand::SetShieldedProtocolVersion {
            protocol_version,
            sync_offset,
        } => {
            if state.offsets.get("shielded").copied().unwrap_or(0) != *sync_offset {
                return Err(MidnightRuntimeError::SyncGap);
            }
            state.wallet_state = state
                .wallet_state
                .set_shielded_protocol_version(*protocol_version)?;
            return complete_json(to_json(&serde_json::json!({
                "protocolVersion": protocol_version,
            }))?);
        }
        RuntimeCommand::CreateDustSpendRequest => {
            let (body, utxo_count) = state
                .wallet_state
                .dust_spend_request(&state.secrets.dust_seed)?;
            return complete_json(to_json(&serde_json::json!({
                "requestBase64": encode_base64(&body),
                "utxoCount": utxo_count,
            }))?);
        }
        RuntimeCommand::CreateDustCommitmentRequest {
            sync_offset,
            raw_base64,
        } => {
            if state.offsets.get("dust").copied().unwrap_or(0) != *sync_offset {
                return Err(MidnightRuntimeError::SyncGap);
            }
            let spend_payload = decode_base64(raw_base64)?;
            let response = state.wallet_state.dust_commitment_request(
                &spend_payload,
                *sync_offset,
                &state.secrets.dust_seed,
            )?;
            return match response {
                DustCommitmentRequest::Ahead => complete_json(to_json(&serde_json::json!({
                    "status": "ahead",
                }))?),
                DustCommitmentRequest::Unchanged => complete_json(to_json(&serde_json::json!({
                    "status": "unchanged",
                }))?),
                DustCommitmentRequest::Rebuild(body) => {
                    complete_json(to_json(&serde_json::json!({
                        "status": "rebuild",
                        "requestBase64": encode_base64(&body),
                    }))?)
                }
            };
        }
        RuntimeCommand::ApplyDustSpendResolution {
            sync_offset,
            raw_base64,
            result_base64,
        } => {
            if state.offsets.get("dust").copied().unwrap_or(0) != *sync_offset {
                return Err(MidnightRuntimeError::SyncGap);
            }
            let spend_payload = decode_base64(raw_base64)?;
            let commitment_response = decode_base64(result_base64)?;
            state.wallet_state = state.wallet_state.apply_dust_spend_resolution(
                &spend_payload,
                &commitment_response,
                *sync_offset,
                &state.secrets.dust_seed,
            )?;
            return complete_json(to_json(&serde_json::json!({
                "status": "applied",
            }))?);
        }
        _ => {}
    }
    Err(MidnightRuntimeError::InvalidArgument)
}
