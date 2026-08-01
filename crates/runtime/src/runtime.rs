use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use midnight_base_crypto::schnorr::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use super::{
    MidnightRuntimeError, SOURCE_REVISION, WalletAddressMaterial, codec,
    derive_wallet_address_material_inner, serializable_hex, transaction,
    wallet_state::{
        DappIntentBuildInput, DappIntentInput, DappTransactionOutput, DustBalanceInput,
        DustCommitmentRequest, LegacyExportContext, LegacyWalletState, NativeWalletState,
        RestoreContext, WalletBalanceSnapshot,
    },
};
use midnight_transient_crypto::proofs::ProvingKeyMaterial;

include!("runtime/types.rs");

include!("runtime/registry_types.rs");

include!("runtime/registry.rs");

include!("runtime/checkpoint.rs");

include!("runtime/codec.rs");

include!("runtime/operation_helpers.rs");

include!("runtime/session.rs");

#[uniffi::export]
pub fn begin_command(
    session_id: u64,
    generation: u64,
    mut command_json: String,
) -> Result<String, MidnightRuntimeError> {
    let result = (|| {
        let mut command: RuntimeCommand = serde_json::from_str(&command_json)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let session = session_for_handle(session_id, generation)?;
        let mut state = lock_session(&session)?;
        include!("runtime/commands/session_and_sync.rs");
        include!("runtime/commands/codecs.rs");
        include!("runtime/commands/dapp.rs");
        include!("runtime/commands/transfers.rs");
        include!("runtime/commands/finalization.rs");
        include!("runtime/commands/deploy_and_balance.rs");
        include!("runtime/commands/submission_and_queries.rs");
        Err(MidnightRuntimeError::Unavailable)
    })();
    command_json.zeroize();
    result
}

#[uniffi::export]
pub fn resume_operation(
    operation_id: u64,
    generation: u64,
    mut network_result_json: Option<String>,
) -> Result<String, MidnightRuntimeError> {
    let mut resumed_session = None;
    let resumed = (|| {
        let result_json = network_result_json
            .as_deref()
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        let result: NetworkResult =
            serde_json::from_str(result_json).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let mut runtime = lock_registry()?;
        let mut operation = match runtime.operations.remove(&operation_id) {
            Some(operation) => operation,
            None if runtime
                .cancelled_operations
                .remove(&(operation_id, generation)) =>
            {
                return Err(MidnightRuntimeError::Cancelled);
            }
            None => return Err(MidnightRuntimeError::StaleSession),
        };
        if operation.generation != generation {
            runtime.operations.insert(operation_id, operation);
            return Err(MidnightRuntimeError::StaleSession);
        }
        let session = runtime.sessions.get(&operation.session_id).cloned();
        drop(runtime);
        let session = session.ok_or(MidnightRuntimeError::StaleSession)?;
        resumed_session = Some(Arc::clone(&session));
        let mut state = lock_session(&session)?;
        if state.generation != generation || state.closing {
            return Err(MidnightRuntimeError::StaleSession);
        }
        if state.active_operation != Some(operation_id) {
            return Err(MidnightRuntimeError::StaleSession);
        }
        if result.effect_id != operation.effect_id {
            drop(state);
            lock_registry()?.operations.insert(operation_id, operation);
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        include!("runtime/resume/dapp.rs");
        include!("runtime/resume/generate_dust.rs");
        include!("runtime/resume/balance.rs");
        include!("runtime/resume/transaction.rs");
        include!("runtime/resume/finalize.rs")
    })();
    if resumed.is_err()
        && let Some(session) = resumed_session
    {
        clear_orphaned_active_operation(&session, operation_id);
    }
    if let Some(result_json) = network_result_json.as_mut() {
        result_json.zeroize();
    }
    resumed
}

#[uniffi::export]
pub fn cancel_operation(operation_id: u64, generation: u64) -> Result<(), MidnightRuntimeError> {
    let mut runtime = lock_registry()?;
    let operation = runtime
        .operations
        .remove(&operation_id)
        .ok_or(MidnightRuntimeError::Cancelled)?;
    if operation.generation != generation {
        runtime.operations.insert(operation_id, operation);
        return Err(MidnightRuntimeError::StaleSession);
    }
    let submission_hash = match &operation.kind {
        PendingOperationKind::SubmitFinalized { transaction_hash } => {
            Some(transaction_hash.clone())
        }
        PendingOperationKind::FinalizeTransactionProof { .. }
        | PendingOperationKind::FinalizeTransactionBalance { .. }
        | PendingOperationKind::DappIntentProof { .. }
        | PendingOperationKind::GenerateDustProof { .. }
        | PendingOperationKind::Balance { .. } => None,
    };
    runtime.remember_cancelled((operation_id, generation));
    let session = runtime.sessions.get(&operation.session_id).cloned();
    drop(runtime);
    if let Some(session) = session {
        let mut state = lock_session(&session)?;
        if state.generation == generation {
            clear_active_operation(&mut state, operation_id);
            if let Some(transaction_hash) = submission_hash
                && let Some(pending) = state.pending_submissions.get_mut(&transaction_hash)
            {
                pending.status = SubmissionStatus::StatusUnknown;
            }
        }
    }
    Ok(())
}

#[uniffi::export]
pub fn close_wallet_session(session_id: u64, generation: u64) -> Result<(), MidnightRuntimeError> {
    let session = {
        let mut runtime = lock_registry()?;
        let session = runtime
            .sessions
            .get(&session_id)
            .cloned()
            .ok_or(MidnightRuntimeError::StaleSession)?;
        {
            let state = lock_session(&session)?;
            if state.generation != generation || state.closing {
                return Err(MidnightRuntimeError::StaleSession);
            }
        }
        runtime.sessions.remove(&session_id);
        runtime
            .operations
            .retain(|_, operation| operation.session_id != session_id);
        runtime.clear_cancelled_generation(generation);
        session
    };
    let mut state = lock_session(&session)?;
    state.closing = true;
    state.secrets.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[path = "operations.rs"]
    mod operations;

    include!("runtime/tests/sanitized.rs");
    include!("runtime/tests/hardening.rs");
    include!("runtime/tests/coverage.rs");
}
