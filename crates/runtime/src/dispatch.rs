//! Routes calls for the `stagenet` network (Ledger 9) to the `runtime-v9` engine.
//! Its ids start at `v9::ID_BASE`, so a session or operation id picks the engine.

use midnight_mobile_runtime_v9 as v9;

use crate::{MidnightRuntimeError, RuntimeSessionHandle};

type V9Error = v9::MidnightRuntimeError;

pub(crate) fn is_v9(id: u64) -> bool {
    id >= v9::ID_BASE
}

pub(crate) fn is_v9_network(config_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(config_json)
        .is_ok_and(|config| config["networkId"] == "stagenet")
}

/// Errors have the same codes in both engines.
fn error(error: V9Error) -> MidnightRuntimeError {
    match error {
        V9Error::Unavailable => MidnightRuntimeError::Unavailable,
        V9Error::InvalidArgument => MidnightRuntimeError::InvalidArgument,
        V9Error::StaleSession => MidnightRuntimeError::StaleSession,
        V9Error::Cancelled => MidnightRuntimeError::Cancelled,
        V9Error::StateIncompatible => MidnightRuntimeError::StateIncompatible,
        V9Error::SyncGap => MidnightRuntimeError::SyncGap,
        V9Error::ProofFailed => MidnightRuntimeError::ProofFailed,
        V9Error::InsufficientDust => MidnightRuntimeError::InsufficientDust,
        V9Error::InsufficientFunds => MidnightRuntimeError::InsufficientFunds,
        V9Error::BalanceApprovalChanged => MidnightRuntimeError::BalanceApprovalChanged,
        V9Error::UnsupportedTransaction => MidnightRuntimeError::UnsupportedTransaction,
        V9Error::SubmissionStatusUnknown => MidnightRuntimeError::SubmissionStatusUnknown,
        V9Error::InvalidLength => MidnightRuntimeError::InvalidLength,
        V9Error::DecodeFailed => MidnightRuntimeError::DecodeFailed,
        V9Error::NativeInternal => MidnightRuntimeError::NativeInternal,
    }
}

pub(crate) fn open_wallet_session(
    config_json: String,
    night_external_key: Vec<u8>,
    zswap_seed: Vec<u8>,
    dust_seed: Vec<u8>,
    checkpoint: Option<Vec<u8>>,
) -> Result<RuntimeSessionHandle, MidnightRuntimeError> {
    v9::open_wallet_session(
        config_json,
        night_external_key,
        zswap_seed,
        dust_seed,
        checkpoint,
    )
    .map(|handle| RuntimeSessionHandle {
        id: handle.id,
        generation: handle.generation,
    })
    .map_err(error)
}

pub(crate) fn apply_sync_batch(
    session_id: u64,
    generation: u64,
    stream: String,
    from_offset: u64,
    to_offset: u64,
    payloads: Vec<Vec<u8>>,
) -> Result<String, MidnightRuntimeError> {
    v9::apply_sync_batch(
        session_id,
        generation,
        stream,
        from_offset,
        to_offset,
        payloads,
    )
    .map_err(error)
}

pub(crate) fn get_wallet_snapshot(
    session_id: u64,
    generation: u64,
) -> Result<String, MidnightRuntimeError> {
    v9::get_wallet_snapshot(session_id, generation).map_err(error)
}

pub(crate) fn export_wallet_checkpoint(
    session_id: u64,
    generation: u64,
) -> Result<Vec<u8>, MidnightRuntimeError> {
    v9::export_wallet_checkpoint(session_id, generation).map_err(error)
}

pub(crate) fn sign_gateway_challenge(
    session_id: u64,
    generation: u64,
    challenge: Vec<u8>,
) -> Result<String, MidnightRuntimeError> {
    v9::sign_gateway_challenge(session_id, generation, challenge).map_err(error)
}

pub(crate) fn begin_command(
    session_id: u64,
    generation: u64,
    command_json: String,
) -> Result<String, MidnightRuntimeError> {
    v9::begin_command(session_id, generation, command_json).map_err(error)
}

pub(crate) fn resume_operation(
    operation_id: u64,
    generation: u64,
    network_result_json: Option<String>,
) -> Result<String, MidnightRuntimeError> {
    v9::resume_operation(operation_id, generation, network_result_json).map_err(error)
}

pub(crate) fn cancel_operation(
    operation_id: u64,
    generation: u64,
) -> Result<(), MidnightRuntimeError> {
    v9::cancel_operation(operation_id, generation).map_err(error)
}

pub(crate) fn close_wallet_session(
    session_id: u64,
    generation: u64,
) -> Result<(), MidnightRuntimeError> {
    v9::close_wallet_session(session_id, generation).map_err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(network: &str) -> String {
        serde_json::json!({
            "networkId": network,
            "walletFingerprint": "fp",
            "unshieldedAddress": "addr",
        })
        .to_string()
    }

    #[test]
    fn only_stagenet_selects_ledger_9() {
        assert!(is_v9_network(&config("stagenet")));
        for network in ["mainnet", "preprod", "preview", "undeployed"] {
            assert!(!is_v9_network(&config(network)));
        }
        assert!(!is_v9_network("not json"));
    }

    #[test]
    fn ids_route_by_engine_range() {
        assert!(!is_v9(1));
        assert!(!is_v9(v9::ID_BASE - 1));
        assert!(is_v9(v9::ID_BASE + 1));
    }
}
