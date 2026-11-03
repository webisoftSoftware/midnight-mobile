//! Hook for one host crate that adds commands to the runtime.
//! Not a stable API: it exists for the 1AM app and is hidden from the docs.

use std::any::Any;

use midnight_ledger::structure::LedgerParameters;
use serde_json::{Map, Value};

use super::*;

pub use super::operation_helpers::{
    accept_proof_batch, decode_proof_batch, take_proving_key_material,
};
pub use super::types::{NetworkError, NetworkResult, OperationHandle, RuntimeProvingKeyMaterial};
pub use super::wire_codec::{decode_base64, encode_base64, to_json};
pub use crate::codec::decompress_gzip;
pub use crate::transaction::{
    BalanceProgress, FinalizedTransaction, MAX_PROOF_BATCH, RemoteProofKeyMaterials,
    RemoteProofKind, RemoteProofRequest, RemoteProofResponses, UnsealedProofProgress,
    advance_unproven_transaction_unsealed_with_materials,
    advance_unproven_transaction_with_materials, decode_ledger_parameters,
    validate_unproven_transaction,
};
pub use crate::wallet_state::{NativeWalletState, ShieldedMintContext, WalletBalanceSnapshot};

pub const SOURCE_REVISION: &str = crate::SOURCE_REVISION;

pub fn serializable_hex<T: midnight_serialize::Serializable + ?Sized>(
    value: &T,
) -> Result<String, MidnightRuntimeError> {
    crate::serializable_hex(value)
}

/// The host's state for one operation.
pub type ExtensionOp = Box<dyn Any + Send>;

/// One network effect the host wants the app to run.
pub struct ExtensionEffect {
    pub effect: &'static str,
    pub endpoint_role: &'static str,
    pub body_base64: String,
}

/// The next step of an operation: its state and the effects to run.
/// Several effects form one batch, resumed together.
pub struct ExtensionStep {
    pub op: ExtensionOp,
    pub effects: Vec<ExtensionEffect>,
}

pub enum ExtensionOutcome {
    /// Run these effects, then resume with the new state.
    Continue(ExtensionStep),
    /// Done. The string is the result JSON.
    Complete(String),
    /// The last result is a balance-service result: the SDK validates it, commits
    /// `proposed_state` and completes with its fields plus `extra`.
    Finalize {
        proposed_state: Option<Box<NativeWalletState>>,
        expected_identifiers: Vec<String>,
        extra: Map<String, Value>,
    },
    /// Put the operation back unchanged and fail this call.
    Retry(ExtensionOp, MidnightRuntimeError),
}

pub trait Extension: Send + Sync + 'static {
    /// Starts a command the SDK does not know. `None` means "not mine".
    fn begin(
        &self,
        ctx: &mut SessionContext<'_>,
        kind: &str,
        json: &str,
    ) -> Option<Result<ExtensionStep, MidnightRuntimeError>>;

    /// Resumes an operation from `begin`. `results` holds one result, or one per
    /// effect of a batch. An error ends the operation.
    fn resume(
        &self,
        ctx: &mut SessionContext<'_>,
        op: ExtensionOp,
        effect_id: &str,
        results: &[NetworkResult],
    ) -> Result<ExtensionOutcome, MidnightRuntimeError>;

    /// The proof requests the operation waits on, so the SDK can check batch results.
    fn pending_requests<'a>(&self, _op: &'a (dyn Any + Send)) -> Option<&'a [RemoteProofRequest]> {
        None
    }
}

static EXTENSION: OnceLock<Box<dyn Extension>> = OnceLock::new();

/// Registers the host extension once. Returns false if one is already set.
pub fn set_extension(extension: impl Extension) -> bool {
    EXTENSION.set(Box::new(extension)).is_ok()
}

/// The host's view of a session, valid for one call.
pub struct SessionContext<'a>(&'a mut SessionState);

impl SessionContext<'_> {
    pub fn network_id(&self) -> &str {
        &self.0.config.network_id
    }

    pub fn address_material(&self) -> &WalletAddressMaterial {
        &self.0.address_material
    }

    pub fn zswap_seed(&self) -> &[u8] {
        &self.0.secrets.zswap_seed
    }

    pub fn caught_up_streams(&self) -> &HashSet<String> {
        &self.0.caught_up_streams
    }

    pub fn balances(&self) -> Result<WalletBalanceSnapshot, MidnightRuntimeError> {
        self.0.wallet_state.balances()
    }

    pub fn shielded_mint_context(&self) -> Result<ShieldedMintContext, MidnightRuntimeError> {
        self.0
            .wallet_state
            .shielded_mint_context(&self.0.secrets.zswap_seed)
    }

    /// Returns the state with the minted coin tracked, and its output index.
    pub fn watch_shielded_mint(
        &self,
        raw_coin: &[u8],
        expected_output_index: u64,
    ) -> Result<(NativeWalletState, u64), MidnightRuntimeError> {
        self.0.wallet_state.watch_shielded_mint(
            raw_coin,
            expected_output_index,
            &self.0.secrets.zswap_seed,
        )
    }

    /// Adds the wallet's token side to a proved, unbound call for a sponsored (DUST-free)
    /// fee. Only an in-place rewrite is supported: a plan that needs a balancing
    /// transaction fails. Returns the rewritten transaction and the proposed state.
    pub fn plan_sponsored_balance(
        &self,
        proven_raw: &[u8],
        ledger_parameters: &LedgerParameters,
    ) -> Result<(Vec<u8>, NativeWalletState), MidnightRuntimeError> {
        let state = &*self.0;
        let current_time_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?
            .as_secs();
        let plan = state.wallet_state.plan_balance(BalanceRequest {
            network_id: &state.config.network_id,
            original_raw: proven_raw,
            sealed: false,
            ledger_parameters,
            fee_blocks_margin: 0,
            additional_fee_overhead: 0,
            fee_mode: FeeMode::Sponsored,
            materialize: true,
            night_external_key: &state.secrets.night_external_key,
            zswap_seed: &state.secrets.zswap_seed,
            dust_seed: &state.secrets.dust_seed,
            current_time_seconds,
            ttl_seconds: current_time_seconds + 30 * 60,
        })?;
        if plan.balancing_raw.is_some() {
            return Err(MidnightRuntimeError::UnsupportedTransaction);
        }
        let base = plan.base_raw.ok_or(MidnightRuntimeError::NativeInternal)?;
        Ok((base, plan.proposed))
    }
}

/// Proof requests as network effects for the proof server.
pub fn proof_effects(requests: &[RemoteProofRequest]) -> Vec<ExtensionEffect> {
    bodies_to_effects(proof_step_bodies(requests))
}

pub(super) fn bodies_to_effects(bodies: Vec<(&'static str, String)>) -> Vec<ExtensionEffect> {
    bodies
        .into_iter()
        .map(|(effect, body_base64)| ExtensionEffect {
            effect,
            endpoint_role: "proof",
            body_base64,
        })
        .collect()
}

pub(super) fn effects_step(
    handle: OperationHandle,
    effect_id: &str,
    effects: Vec<ExtensionEffect>,
) -> Result<OperationStep, MidnightRuntimeError> {
    let total = effects.len();
    let first = effects.first().ok_or(MidnightRuntimeError::ProofFailed)?;
    let list = (total > 1).then(|| {
        effects
            .iter()
            .enumerate()
            .map(|(index, effect)| OperationEffect {
                effect_id: proof_effect_id(effect_id, index, total),
                effect: effect.effect,
                endpoint_role: effect.endpoint_role,
                body_base64: effect.body_base64.clone(),
            })
            .collect()
    });
    Ok(OperationStep {
        kind: "network",
        operation: Some(handle),
        effect_id: Some(proof_effect_id(effect_id, 0, total)),
        effect: Some(first.effect),
        endpoint_role: Some(first.endpoint_role),
        body_base64: Some(first.body_base64.clone()),
        effects: list,
        result_json: None,
    })
}

pub(super) fn pending_requests(op: &(dyn Any + Send)) -> Option<&[RemoteProofRequest]> {
    EXTENSION.get()?.pending_requests(op)
}

/// Every `RuntimeCommand` kind. Anything else belongs to the extension.
pub(super) const SDK_KINDS: &[&str] = &[
    "signData",
    "createCheckPayload",
    "parseCheckResult",
    "createProvingPayload",
    "canonicalizeTransaction",
    "createSyncRequest",
    "deriveShieldedMintContext",
    "watchShieldedMint",
    "createShieldedSpentRequest",
    "applyShieldedSpentResponse",
    "setShieldedProtocolVersion",
    "createDustSpendRequest",
    "createDustCommitmentRequest",
    "applyDustSpendResolution",
    "transfer",
    "dappTransfer",
    "dappIntent",
    "generateDust",
    "previewBalance",
    "balanceUnsealed",
    "balanceSealed",
    "finalizeUnprovenTransaction",
    "submitFinalized",
];

/// The command kind when it is not an SDK kind. Read before the typed parse, which
/// rejects unknown kinds.
pub(super) fn host_kind(json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Kind {
        kind: String,
    }
    let Kind { kind } = serde_json::from_str(json).ok()?;
    (!SDK_KINDS.contains(&kind.as_str())).then_some(kind)
}

pub(super) fn begin(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    mut state: MutexGuard<'_, SessionState>,
    kind: &str,
    json: &str,
) -> Result<String, MidnightRuntimeError> {
    let extension = EXTENSION
        .get()
        .ok_or(MidnightRuntimeError::UnsupportedCommand)?;
    let ExtensionStep { op, effects } = extension
        .begin(&mut SessionContext(&mut state), kind, json)
        .ok_or(MidnightRuntimeError::UnsupportedCommand)??;
    drop(state);
    let (handle, effect_id) = register_operation(
        session_id,
        generation,
        session,
        PendingOperationKind::Extension(op),
    )?;
    to_json(&effects_step(handle, &effect_id, effects)?)
}

pub(super) fn resume(
    operation_id: u64,
    generation: u64,
    results: &[NetworkResult],
    operation: PendingOperation,
    mut state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    let PendingOperation {
        session_id,
        effect_id,
        kind,
        ..
    } = operation;
    let PendingOperationKind::Extension(op) = kind else {
        return Err(MidnightRuntimeError::NativeInternal);
    };
    let extension = EXTENSION
        .get()
        .ok_or(MidnightRuntimeError::NativeInternal)?;
    let handle = OperationHandle {
        id: operation_id,
        generation,
    };
    let outcome = extension.resume(&mut SessionContext(&mut state), op, &effect_id, results)?;
    let complete = |result_json| {
        to_json(&OperationStep {
            kind: "complete",
            operation: Some(handle.clone()),
            effect_id: None,
            effect: None,
            endpoint_role: None,
            body_base64: None,
            effects: None,
            result_json: Some(result_json),
        })
    };
    match outcome {
        ExtensionOutcome::Continue(ExtensionStep { op, effects }) => {
            let effect_id = next_effect_id(generation, operation_id, &effect_id);
            let step = effects_step(handle, &effect_id, effects)?;
            drop(state);
            lock_registry()?.operations.insert(
                operation_id,
                PendingOperation {
                    generation,
                    session_id,
                    effect_id,
                    kind: PendingOperationKind::Extension(op),
                },
            );
            to_json(&step)
        }
        ExtensionOutcome::Complete(result_json) => {
            clear_active_operation(&mut state, operation_id);
            complete(result_json)
        }
        ExtensionOutcome::Finalize {
            proposed_state,
            expected_identifiers,
            extra,
        } => {
            let first = results
                .first()
                .ok_or(MidnightRuntimeError::InvalidArgument)?;
            let decoded = decode_balance_service_result(
                first,
                &state.config.network_id,
                &expected_identifiers,
            );
            clear_active_operation(&mut state, operation_id);
            let (response, finalized, hash) = decoded?;
            if let Some(proposed_state) = proposed_state {
                state.wallet_state = *proposed_state;
            }
            let mut result: Map<String, Value> = serde_json::from_str(
                &balance_service_transaction_result(&response, &finalized, &hash)?,
            )
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            result.extend(extra);
            complete(to_json(&result)?)
        }
        ExtensionOutcome::Retry(op, error) => {
            drop(state);
            lock_registry()?.operations.insert(
                operation_id,
                PendingOperation {
                    generation,
                    session_id,
                    effect_id,
                    kind: PendingOperationKind::Extension(op),
                },
            );
            Err(error)
        }
    }
}
