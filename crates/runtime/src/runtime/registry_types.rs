impl SessionSecrets {
    fn clear(&mut self) {
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
struct BatchKey {
    stream: String,
    from_offset: u64,
    to_offset: u64,
}

struct SessionState {
    generation: u64,
    config: WalletSessionConfig,
    secrets: SessionSecrets,
    address_material: WalletAddressMaterial,
    wallet_state: NativeWalletState,
    offsets: HashMap<String, u64>,
    applied_batches: HashMap<BatchKey, String>,
    seen_streams: HashSet<String>,
    caught_up_streams: HashSet<String>,
    pending_submissions: HashMap<String, PendingSubmission>,
    active_operation: Option<u64>,
    closing: bool,
}

enum PendingOperationKind {
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
    fn pending_requests(&self) -> Option<&[transaction::RemoteProofRequest]> {
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
    fn set_pending_requests(&mut self, requests: Vec<transaction::RemoteProofRequest>) {
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

struct PendingOperation {
    generation: u64,
    session_id: u64,
    effect_id: String,
    kind: PendingOperationKind,
}

struct RuntimeRegistry {
    next_id: u64,
    next_generation: u64,
    sessions: HashMap<u64, Arc<Mutex<SessionState>>>,
    operations: HashMap<u64, PendingOperation>,
    cancelled_operations: HashSet<(u64, u64)>,
    cancelled_operation_order: VecDeque<(u64, u64)>,
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
    fn remember_cancelled(&mut self, key: (u64, u64)) {
        if self.cancelled_operations.insert(key) {
            self.cancelled_operation_order.push_back(key);
        }
        while self.cancelled_operation_order.len() > MAX_CANCELLED_OPERATION_TOMBSTONES {
            if let Some(expired) = self.cancelled_operation_order.pop_front() {
                self.cancelled_operations.remove(&expired);
            }
        }
    }

    fn clear_cancelled_generation(&mut self, generation: u64) {
        self.cancelled_operations
            .retain(|(_, candidate)| *candidate != generation);
        self.cancelled_operation_order
            .retain(|(_, candidate)| *candidate != generation);
    }
}
