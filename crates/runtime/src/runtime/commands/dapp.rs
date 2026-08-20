use super::super::*;

pub(super) fn handle(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    mut command: RuntimeCommand,
    mut state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    if let RuntimeCommand::DappTransfer { outputs } = &mut command {
        if !["shielded", "unshielded", "dust"]
            .iter()
            .all(|stream| state.caught_up_streams.contains(*stream))
        {
            return Err(MidnightRuntimeError::Unavailable);
        }
        let outputs = parse_dapp_outputs(std::mem::take(outputs))?;
        let ttl_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?
            .as_secs()
            .checked_add(30 * 60)
            .ok_or(MidnightRuntimeError::NativeInternal)?;
        let (proposed, transaction) = state.wallet_state.build_dapp_transfer_with_rng(
            &state.config.network_id,
            &outputs,
            &state.secrets.night_external_key,
            &state.secrets.zswap_seed,
            ttl_seconds,
            &mut OsRng,
        )?;
        let expected_identifiers =
            transaction::validate_unproven_transaction(&transaction, &state.config.network_id)?;
        let network_id = state.config.network_id.clone();
        reserve_operation(&mut state)?;
        drop(state);
        return start_transaction_finalization(
            session_id,
            generation,
            session,
            TransactionFinalizationInput {
                network_id,
                raw: transaction,
                key_material: transaction::RemoteProofKeyMaterials::default(),
                proposed_state: Some(proposed),
                expected_identifiers,
            },
        );
    }

    if let RuntimeCommand::DappIntent { inputs, outputs } = &mut command {
        if !["shielded", "unshielded", "dust"]
            .iter()
            .all(|stream| state.caught_up_streams.contains(*stream))
        {
            return Err(MidnightRuntimeError::Unavailable);
        }
        let inputs = parse_dapp_inputs(std::mem::take(inputs))?;
        let outputs = parse_dapp_outputs(std::mem::take(outputs))?;
        let ttl_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?
            .as_secs()
            .checked_add(30 * 60)
            .ok_or(MidnightRuntimeError::NativeInternal)?;
        let raw = state.wallet_state.build_dapp_intent_with_rng(
            DappIntentBuildInput {
                network_id: &state.config.network_id,
                inputs: &inputs,
                outputs: &outputs,
                night_external_key: &state.secrets.night_external_key,
                zswap_seed: &state.secrets.zswap_seed,
                ttl_seconds,
            },
            &mut OsRng,
        )?;
        let responses = transaction::RemoteProofResponses::default();
        let progress =
            transaction::advance_unproven_transaction(&raw, &state.config.network_id, &responses)?;
        match progress {
            transaction::BalanceProgress::Complete(finalized) => {
                return complete_json(finalized_transaction_result(&finalized)?);
            }
            transaction::BalanceProgress::Network(pending_requests) => {
                let bodies = proof_step_bodies(&pending_requests);
                drop(state);
                let (operation, effect_id) = register_operation(
                    session_id,
                    generation,
                    session,
                    PendingOperationKind::DappIntentProof {
                        raw,
                        responses,
                        pending_requests,
                    },
                )?;
                return to_json(&proof_step_from_bodies(operation, &effect_id, bodies)?);
            }
        }
    }
    Err(MidnightRuntimeError::InvalidArgument)
}
