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

// This UniFFI record stays at the runtime module boundary so its stable public
// type identity is independent of the private implementation layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, uniffi::Record)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSessionHandle {
    pub id: u64,
    pub generation: u64,
}

mod checkpoint;
mod commands;
mod operation_helpers;
mod registry;
mod registry_types;
mod resume;
mod session;
mod types;
#[path = "runtime/codec.rs"]
mod wire_codec;

use checkpoint::*;
use operation_helpers::*;
use registry::*;
use registry_types::*;
use types::*;
use wire_codec::*;

// Keep the exported functions in this module so UniFFI's public metadata remains
// stable while the session implementation lives in its focused submodule.
#[uniffi::export]
pub fn open_wallet_session(
    config_json: String,
    night_external_key: Vec<u8>,
    zswap_seed: Vec<u8>,
    dust_seed: Vec<u8>,
    checkpoint: Option<Vec<u8>>,
) -> Result<RuntimeSessionHandle, MidnightRuntimeError> {
    session::open_wallet_session(
        config_json,
        night_external_key,
        zswap_seed,
        dust_seed,
        checkpoint,
    )
}

#[uniffi::export]
pub fn apply_sync_batch(
    session_id: u64,
    generation: u64,
    stream: String,
    from_offset: u64,
    to_offset: u64,
    payloads: Vec<Vec<u8>>,
) -> Result<String, MidnightRuntimeError> {
    session::apply_sync_batch(
        session_id,
        generation,
        stream,
        from_offset,
        to_offset,
        payloads,
    )
}

#[uniffi::export]
pub fn get_wallet_snapshot(
    session_id: u64,
    generation: u64,
) -> Result<String, MidnightRuntimeError> {
    session::get_wallet_snapshot(session_id, generation)
}

#[uniffi::export]
pub fn export_wallet_checkpoint(
    session_id: u64,
    generation: u64,
) -> Result<Vec<u8>, MidnightRuntimeError> {
    session::export_wallet_checkpoint(session_id, generation)
}

#[uniffi::export]
pub fn begin_command(
    session_id: u64,
    generation: u64,
    mut command_json: String,
) -> Result<String, MidnightRuntimeError> {
    let result = (|| {
        let command: RuntimeCommand = serde_json::from_str(&command_json)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let session = session_for_handle(session_id, generation)?;
        let state = lock_session(&session)?;
        commands::begin_command_kind(session_id, generation, &session, command, state)
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
        let results = serde_json::from_str::<NetworkResults>(result_json)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?
            .into_vec();
        if results.is_empty() || results.len() > transaction::MAX_PROOF_BATCH {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let mut runtime = lock_registry()?;
        let operation = match runtime.operations.remove(&operation_id) {
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
        let state = lock_session(&session)?;
        if state.generation != generation || state.closing {
            return Err(MidnightRuntimeError::StaleSession);
        }
        if state.active_operation != Some(operation_id) {
            return Err(MidnightRuntimeError::StaleSession);
        }
        // A single result keeps the pre-batching contract: its effect id must equal the
        // operation's. A multi-result payload only makes sense for a batched proof round,
        // and `decode_proof_batch` validates its ids against the outstanding requests.
        let result = results
            .first()
            .ok_or(MidnightRuntimeError::InvalidArgument)?
            .clone();
        let outstanding = operation
            .kind
            .pending_requests()
            .map_or(0, <[transaction::RemoteProofRequest]>::len);
        let mismatched = if results.len() > 1 {
            outstanding < 2
        } else {
            result.effect_id != operation.effect_id
        };
        if mismatched {
            drop(state);
            lock_registry()?.operations.insert(operation_id, operation);
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        resume::resume_operation_kind(
            operation_id,
            generation,
            &results,
            &result,
            operation,
            state,
        )
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
#[path = "runtime/tests/mod.rs"]
mod tests;
