use super::super::*;

use crate::wallet_state::BalancePlan;

/// The parts of a balance request that are identical for a preview and for the
/// execution that follows it.
struct BalanceArguments<'a> {
    sealed: bool,
    raw_base64: &'a str,
    ledger_parameters_base64: &'a str,
    fee_blocks_margin: u64,
    additional_fee_overhead: &'a str,
    fee_mode: &'a str,
    approved_manifest: Option<&'a BalanceManifest>,
}

fn arguments(command: &RuntimeCommand) -> Option<BalanceArguments<'_>> {
    match command {
        RuntimeCommand::PreviewBalance {
            raw_base64,
            sealed,
            ledger_parameters_base64,
            fee_blocks_margin,
            additional_fee_overhead,
            fee_mode,
        } => Some(BalanceArguments {
            sealed: *sealed,
            raw_base64,
            ledger_parameters_base64,
            fee_blocks_margin: *fee_blocks_margin,
            additional_fee_overhead,
            fee_mode,
            approved_manifest: None,
        }),
        RuntimeCommand::BalanceUnsealed {
            raw_base64,
            ledger_parameters_base64,
            fee_blocks_margin,
            additional_fee_overhead,
            fee_mode,
            approved_manifest,
        } => Some(BalanceArguments {
            sealed: false,
            raw_base64,
            ledger_parameters_base64,
            fee_blocks_margin: *fee_blocks_margin,
            additional_fee_overhead,
            fee_mode,
            approved_manifest: Some(approved_manifest),
        }),
        RuntimeCommand::BalanceSealed {
            raw_base64,
            ledger_parameters_base64,
            fee_blocks_margin,
            additional_fee_overhead,
            fee_mode,
            approved_manifest,
        } => Some(BalanceArguments {
            sealed: true,
            raw_base64,
            ledger_parameters_base64,
            fee_blocks_margin: *fee_blocks_margin,
            additional_fee_overhead,
            fee_mode,
            approved_manifest: Some(approved_manifest),
        }),
        _ => None,
    }
}

fn fee_mode(value: &str) -> Result<FeeMode, MidnightRuntimeError> {
    match value {
        "localDust" => Ok(FeeMode::LocalDust),
        "sponsored" => Ok(FeeMode::Sponsored),
        _ => Err(MidnightRuntimeError::InvalidArgument),
    }
}

/// Runs the planner against a clone of the session's wallet state.
///
/// The returned plan is never applied here: a preview must not reserve a coin,
/// and finalized submission stays the owner of pending-input registration.
fn plan(
    state: &MutexGuard<'_, SessionState>,
    arguments: &BalanceArguments<'_>,
    original_raw: &[u8],
) -> Result<BalancePlan, MidnightRuntimeError> {
    let parameters_raw = decode_base64(arguments.ledger_parameters_base64)?;
    let parameters = transaction::decode_ledger_parameters(&parameters_raw)?;
    if arguments.fee_blocks_margin > 64 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let fee_blocks_margin = usize::try_from(arguments.fee_blocks_margin)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let additional_fee_overhead = arguments
        .additional_fee_overhead
        .parse::<u128>()
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if additional_fee_overhead.to_string() != *arguments.additional_fee_overhead {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let current_time_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?
        .as_secs();
    let ttl_seconds = current_time_seconds
        .checked_add(30 * 60)
        .ok_or(MidnightRuntimeError::NativeInternal)?;
    state.wallet_state.plan_balance(BalanceRequest {
        network_id: &state.config.network_id,
        original_raw,
        sealed: arguments.sealed,
        ledger_parameters: &parameters,
        fee_blocks_margin,
        additional_fee_overhead,
        fee_mode: fee_mode(arguments.fee_mode)?,
        night_external_key: &state.secrets.night_external_key,
        zswap_seed: &state.secrets.zswap_seed,
        dust_seed: &state.secrets.dust_seed,
        current_time_seconds,
        ttl_seconds,
    })
}

pub(super) fn handle(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    command: RuntimeCommand,
    state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    let Some(arguments) = arguments(&command) else {
        return Err(MidnightRuntimeError::InvalidArgument);
    };
    // Balancing spends the wallet's own coins and DUST, so every stream must be
    // at the indexer tip before any selection decision is made.
    if !["shielded", "unshielded", "dust"]
        .iter()
        .all(|stream| state.caught_up_streams.contains(*stream))
    {
        return Err(MidnightRuntimeError::Unavailable);
    }
    let original_raw = decode_base64(arguments.raw_base64)?;
    let BalancePlan {
        proposed,
        base_raw,
        balancing_raw,
        manifest,
    } = plan(&state, &arguments, &original_raw)?;
    // Dropping the proposed state is what makes a preview side-effect free:
    // no coin is reserved here, and finalized submission stays the owner of
    // pending-input registration.
    drop(proposed);

    let Some(approved) = arguments.approved_manifest else {
        // A preview has no network effects, so it completes inline. Wrap it in a
        // completion step so the host decodes it like every other result.
        return complete_json(to_json(&BalancePreviewResult {
            manifest_digest: manifest.digest(),
            manifest,
        })?);
    };
    // Fail closed: the transaction, the token contributions, and the wallet
    // state must all still match what the user approved, and the DUST cost may
    // only have gone down.
    approved.authorizes(&manifest)?;

    let base_raw = base_raw.unwrap_or(original_raw);
    // The base is only rewritten for an unsealed transaction, and a rewrite
    // keeps it unsealed, so the sealed flag carries through unchanged.
    let Some(balancing_raw) = balancing_raw else {
        let finalized = transaction::finalize_balance_original(
            &base_raw,
            arguments.sealed,
            &state.config.network_id,
        )?;
        return complete_json(finalized_transaction_result(&finalized)?);
    };
    let responses = transaction::RemoteProofResponses::default();
    let requests = match transaction::advance_dust_balance(
        &base_raw,
        arguments.sealed,
        &balancing_raw,
        &state.config.network_id,
        &responses,
    )? {
        transaction::BalanceProgress::Network(requests) => requests,
        transaction::BalanceProgress::Complete(finalized) => {
            return complete_json(finalized_transaction_result(&finalized)?);
        }
    };
    drop(state);
    let bodies = proof_step_bodies(&requests);
    let (operation, effect_id) = register_operation(
        session_id,
        generation,
        session,
        PendingOperationKind::Balance {
            original_raw: base_raw,
            original_sealed: arguments.sealed,
            balancing_raw,
            responses,
            pending_requests: requests,
        },
    )?;
    to_json(&proof_step_from_bodies(operation, &effect_id, bodies)?)
}
