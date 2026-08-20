use super::*;

impl SessionSecrets {
    pub(super) fn clear(&mut self) {
        self.night_external_key.zeroize();
        self.zswap_seed.zeroize();
        self.dust_seed.zeroize();
    }
}

impl Drop for SessionSecrets {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) struct BatchKey {
    pub(super) stream: String,
    pub(super) from_offset: u64,
    pub(super) to_offset: u64,
}

pub(super) struct SessionState {
    pub(super) generation: u64,
    pub(super) config: WalletSessionConfig,
    pub(super) secrets: SessionSecrets,
    pub(super) address_material: WalletAddressMaterial,
    pub(super) wallet_state: NativeWalletState,
    pub(super) offsets: HashMap<String, u64>,
    pub(super) applied_batches: HashMap<BatchKey, String>,
    pub(super) seen_streams: HashSet<String>,
    pub(super) caught_up_streams: HashSet<String>,
    pub(super) pending_submissions: HashMap<String, PendingSubmission>,
    pub(super) active_operation: Option<u64>,
    pub(super) closing: bool,
}

pub(super) enum PendingOperationKind {
    SubmitFinalized {
        transaction_hash: String,
    },
    FinalizeTransactionProof {
        raw: Vec<u8>,
        key_material: transaction::RemoteProofKeyMaterials,
        proposed_state: Option<NativeWalletState>,
        expected_identifiers: Vec<String>,
        responses: transaction::RemoteProofResponses,
        pending_requests: Vec<transaction::RemoteProofRequest>,
    },
    FinalizeTransactionBalance {
        proposed_state: Option<NativeWalletState>,
        expected_identifiers: Vec<String>,
    },
    DappIntentProof {
        raw: Vec<u8>,
        responses: transaction::RemoteProofResponses,
        pending_requests: Vec<transaction::RemoteProofRequest>,
    },
    GenerateDustProof {
        raw: Vec<u8>,
        proposed_state: NativeWalletState,
        expected_identifiers: Vec<String>,
        responses: transaction::RemoteProofResponses,
        pending_requests: Vec<transaction::RemoteProofRequest>,
    },
    Balance {
        original_raw: Vec<u8>,
        original_sealed: bool,
        balancing_raw: Vec<u8>,
        responses: transaction::RemoteProofResponses,
        pending_requests: Vec<transaction::RemoteProofRequest>,
    },
}

impl PendingOperationKind {
    /// The proof requests this operation is currently waiting on, or `None` for the
    /// kinds that never carry proof effects.
    pub(super) fn pending_requests(&self) -> Option<&[transaction::RemoteProofRequest]> {
        match self {
            Self::FinalizeTransactionProof {
                pending_requests, ..
            }
            | Self::DappIntentProof {
                pending_requests, ..
            }
            | Self::GenerateDustProof {
                pending_requests, ..
            }
            | Self::Balance {
                pending_requests, ..
            } => Some(pending_requests),
            Self::SubmitFinalized { .. } | Self::FinalizeTransactionBalance { .. } => None,
        }
    }

    /// Replaces the outstanding requests after a replay discovered the next round. The
    /// displaced requests zeroize their bodies on drop.
    pub(super) fn set_pending_requests(&mut self, requests: Vec<transaction::RemoteProofRequest>) {
        match self {
            Self::FinalizeTransactionProof {
                pending_requests, ..
            }
            | Self::DappIntentProof {
                pending_requests, ..
            }
            | Self::GenerateDustProof {
                pending_requests, ..
            }
            | Self::Balance {
                pending_requests, ..
            } => *pending_requests = requests,
            Self::SubmitFinalized { .. } | Self::FinalizeTransactionBalance { .. } => {}
        }
    }
}

pub(super) struct PendingOperation {
    pub(super) generation: u64,
    pub(super) session_id: u64,
    pub(super) effect_id: String,
    pub(super) kind: PendingOperationKind,
}

pub(super) struct RuntimeRegistry {
    pub(super) next_id: u64,
    pub(super) next_generation: u64,
    pub(super) sessions: HashMap<u64, Arc<Mutex<SessionState>>>,
    pub(super) operations: HashMap<u64, PendingOperation>,
    pub(super) cancelled_operations: HashSet<(u64, u64)>,
    pub(super) cancelled_operation_order: VecDeque<(u64, u64)>,
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self {
            next_id: 1,
            next_generation: 1,
            sessions: HashMap::new(),
            operations: HashMap::new(),
            cancelled_operations: HashSet::new(),
            cancelled_operation_order: VecDeque::new(),
        }
    }
}

impl RuntimeRegistry {
    pub(super) fn remember_cancelled(&mut self, key: (u64, u64)) {
        if self.cancelled_operations.insert(key) {
            self.cancelled_operation_order.push_back(key);
        }
        while self.cancelled_operation_order.len() > MAX_CANCELLED_OPERATION_TOMBSTONES {
            if let Some(expired) = self.cancelled_operation_order.pop_front() {
                self.cancelled_operations.remove(&expired);
            }
        }
    }

    pub(super) fn clear_cancelled_generation(&mut self, generation: u64) {
        self.cancelled_operations
            .retain(|(_, candidate)| *candidate != generation);
        self.cancelled_operation_order
            .retain(|(_, candidate)| *candidate != generation);
    }
}
