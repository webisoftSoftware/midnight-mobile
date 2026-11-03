use super::*;

// Local DUST balancing for wallet-owned transactions.
// With sponsorship off, the wallet builds and proves its fee-paying transaction locally.
// It then merges that transaction with the original and does not call the balance service.
#[derive(Clone)]
pub(super) struct LocalDustBalanceRequest {
    ledger_parameters_raw: Vec<u8>,
    fee_blocks_margin: usize,
    additional_fee_overhead: u128,
}

pub(super) enum LocalDustBalanceStart {
    Complete {
        finalized: transaction::FinalizedTransaction,
        proposed_state: NativeWalletState,
    },
    Pending {
        original_raw: Vec<u8>,
        balancing_raw: Vec<u8>,
        responses: transaction::RemoteProofResponses,
        pending_requests: Vec<transaction::RemoteProofRequest>,
        proposed_state: NativeWalletState,
    },
}

/// Read the local-balancing parameters a command carries.
/// Present parameters select local balancing.
/// Absent parameters use sponsorship.
pub(super) fn parse_local_dust_balance_request(
    ledger_parameters_base64: Option<&str>,
    fee_blocks_margin: Option<u64>,
    additional_fee_overhead: Option<&str>,
) -> Result<Option<LocalDustBalanceRequest>, MidnightRuntimeError> {
    let Some(encoded) = ledger_parameters_base64 else {
        return Ok(None);
    };
    let ledger_parameters_raw = decode_base64(encoded)?;
    // Reject an unusable parameter set before the transaction is proven.
    transaction::decode_ledger_parameters(&ledger_parameters_raw)?;
    let fee_blocks_margin = usize::try_from(
        fee_blocks_margin
            .filter(|value| *value <= 64)
            .ok_or(MidnightRuntimeError::InvalidArgument)?,
    )
    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let additional_fee_overhead_raw =
        additional_fee_overhead.ok_or(MidnightRuntimeError::InvalidArgument)?;
    let additional_fee_overhead = additional_fee_overhead_raw
        .parse::<u128>()
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if additional_fee_overhead.to_string() != additional_fee_overhead_raw {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok(Some(LocalDustBalanceRequest {
        ledger_parameters_raw,
        fee_blocks_margin,
        additional_fee_overhead,
    }))
}

/// Select and spend local DUST for an already finalized transaction.
/// Add the DUST spends to the proposed state. The merged transaction includes them.
pub(super) fn begin_local_dust_balance(
    state: &SessionState,
    request: &LocalDustBalanceRequest,
    proposed_state: NativeWalletState,
    finalized: transaction::FinalizedTransaction,
) -> Result<LocalDustBalanceStart, MidnightRuntimeError> {
    let network_id = state.config.network_id.as_str();
    let parameters = transaction::decode_ledger_parameters(&request.ledger_parameters_raw)?;
    let erased = transaction::decode_balance_original(&finalized.canonical, true, network_id)?;
    let current_time_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?
        .as_secs();
    let ttl_seconds = current_time_seconds
        .checked_add(30 * 60)
        .ok_or(MidnightRuntimeError::NativeInternal)?;
    let (proposed_state, balancing_raw) = proposed_state.build_dust_balance(DustBalanceInput {
        network_id,
        original: &erased,
        ledger_parameters: &parameters,
        fee_blocks_margin: request.fee_blocks_margin,
        additional_fee_overhead: request.additional_fee_overhead,
        dust_seed: &state.secrets.dust_seed,
        current_time_seconds,
        ttl_seconds,
    })?;
    let Some(balancing_raw) = balancing_raw else {
        return Ok(LocalDustBalanceStart::Complete {
            finalized,
            proposed_state,
        });
    };
    let responses = transaction::RemoteProofResponses::default();
    match transaction::advance_dust_balance(
        &finalized.canonical,
        true,
        &balancing_raw,
        network_id,
        &responses,
    )? {
        transaction::BalanceProgress::Complete(merged) => Ok(LocalDustBalanceStart::Complete {
            finalized: merged,
            proposed_state,
        }),
        transaction::BalanceProgress::Network(pending_requests) => {
            Ok(LocalDustBalanceStart::Pending {
                original_raw: finalized.canonical,
                balancing_raw,
                responses,
                pending_requests,
                proposed_state,
            })
        }
    }
}

pub(super) struct LocalTransferBalanceInput {
    pub(super) proposed_state: Option<NativeWalletState>,
    pub(super) expected_identifiers: Vec<String>,
    pub(super) finalized: transaction::FinalizedTransaction,
}

/// Starts local balancing after `begin_command` finishes proving a transfer.
/// No operation is registered at this point.
pub(super) fn start_local_transfer_balance(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    request: &LocalDustBalanceRequest,
    input: LocalTransferBalanceInput,
    registered_operation: &mut Option<u64>,
) -> Result<String, MidnightRuntimeError> {
    let LocalTransferBalanceInput {
        proposed_state,
        expected_identifiers,
        finalized,
    } = input;
    let proposed_state = proposed_state.ok_or(MidnightRuntimeError::NativeInternal)?;
    let start = {
        let state = lock_session(session)?;
        if state.generation != generation || state.closing {
            return Err(MidnightRuntimeError::StaleSession);
        }
        begin_local_dust_balance(&state, request, proposed_state, finalized)?
    };
    match start {
        LocalDustBalanceStart::Complete {
            finalized,
            proposed_state,
        } => {
            if !covers_expected_identifiers(&finalized, &expected_identifiers) {
                return Err(MidnightRuntimeError::ProofFailed);
            }
            let result = finalized_transaction_result(&finalized)?;
            let mut state = lock_session(session)?;
            if state.generation != generation || state.closing {
                return Err(MidnightRuntimeError::StaleSession);
            }
            state.wallet_state = proposed_state;
            drop(state);
            clear_operation_reservation(session);
            complete_json(result)
        }
        LocalDustBalanceStart::Pending {
            original_raw,
            balancing_raw,
            responses,
            pending_requests,
            proposed_state,
        } => {
            let bodies = proof_step_bodies(&pending_requests);
            let (operation, effect_id) = register_reserved_operation(
                session_id,
                generation,
                session,
                PendingOperationKind::Balance {
                    original_raw,
                    original_sealed: true,
                    balancing_raw,
                    responses,
                    pending_requests,
                    proposed_state: Some(proposed_state),
                    expected_identifiers,
                },
            )?;
            *registered_operation = Some(operation.id);
            to_json(&proof_step_from_bodies(operation, &effect_id, bodies)?)
        }
    }
}

pub(super) fn covers_expected_identifiers(
    finalized: &transaction::FinalizedTransaction,
    expected_identifiers: &[String],
) -> bool {
    expected_identifiers
        .iter()
        .all(|identifier| finalized.identifiers.contains(identifier))
}
