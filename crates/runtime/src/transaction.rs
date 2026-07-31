use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

use midnight_base_crypto::schnorr::Signature;
use midnight_ledger::structure::{LedgerParameters, ProofMarker, ProofPreimageMarker, Transaction};
use midnight_onchain_runtime::cost_model::INITIAL_COST_MODEL;
use midnight_serialize::{
    Deserializable, Serializable, Tagged, tagged_deserialize, tagged_serialize,
};
use midnight_storage::db::InMemoryDB;
use midnight_transient_crypto::commitment::{Pedersen, PedersenRandomness, PureGeneratorPedersen};
use rand::rngs::OsRng;

use super::MidnightRuntimeError;

mod remote_proof;

use remote_proof::PausingProver;
pub(crate) use remote_proof::{RemoteProofKind, RemoteProofRequest, RemoteProofResponses};

#[cfg(test)]
mod tests;

const MAX_TRANSACTION_BYTES: usize = 64 * 1024 * 1024;

struct ThreadWake(std::thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

pub(crate) struct FinalizedTransaction {
    pub canonical: Vec<u8>,
    pub transaction_hash: String,
    pub identifiers: Vec<String>,
}

pub(crate) enum BalanceProgress {
    Network(RemoteProofRequest),
    Complete(FinalizedTransaction),
}

pub(crate) fn advance_unproven_transaction(
    raw: &[u8],
    expected_network_id: &str,
    responses: &RemoteProofResponses,
) -> Result<BalanceProgress, MidnightRuntimeError> {
    if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES || expected_network_id.is_empty() {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let transaction: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if transaction_network(&transaction) != expected_network_id {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut canonical = Vec::new();
    tagged_serialize(&transaction, &mut canonical)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if canonical != raw {
        return Err(MidnightRuntimeError::InvalidArgument);
    }

    let captured = Arc::new(Mutex::new(BTreeMap::new()));
    let provider = PausingProver {
        responses: Arc::new(responses.clone()),
        captured: Arc::clone(&captured),
    };
    match block_on(transaction.prove(provider, &INITIAL_COST_MODEL)) {
        Ok(proven) => {
            let finalized = proven.seal(OsRng);
            let mut finalized_raw = Vec::new();
            tagged_serialize(&finalized, &mut finalized_raw)
                .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            Ok(BalanceProgress::Complete(validate_finalized_transaction(
                &finalized_raw,
                expected_network_id,
            )?))
        }
        Err(_) => {
            let request = captured
                .lock()
                .map_err(|_| MidnightRuntimeError::NativeInternal)?
                .values()
                .next()
                .cloned()
                .ok_or(MidnightRuntimeError::ProofFailed)?;
            Ok(BalanceProgress::Network(request))
        }
    }
}

pub(crate) fn decode_ledger_parameters(
    raw: &[u8],
) -> Result<LedgerParameters, MidnightRuntimeError> {
    if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let parameters: LedgerParameters =
        tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let mut canonical = Vec::new();
    tagged_serialize(&parameters, &mut canonical)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if canonical != raw {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok(parameters)
}

pub(crate) fn decode_balance_original(
    raw: &[u8],
    sealed: bool,
    expected_network_id: &str,
) -> Result<Transaction<Signature, (), Pedersen, InMemoryDB>, MidnightRuntimeError> {
    if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES || expected_network_id.is_empty() {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let (erased, canonical, network_id) = if sealed {
        let transaction: Transaction<Signature, ProofMarker, PureGeneratorPedersen, InMemoryDB> =
            tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let network_id = transaction_network(&transaction).to_owned();
        let mut canonical = Vec::new();
        tagged_serialize(&transaction, &mut canonical)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;
        (transaction.erase_proofs(), canonical, network_id)
    } else {
        let transaction: Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB> =
            tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let network_id = transaction_network(&transaction).to_owned();
        let mut canonical = Vec::new();
        tagged_serialize(&transaction, &mut canonical)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;
        (transaction.erase_proofs(), canonical, network_id)
    };
    if canonical != raw || network_id != expected_network_id {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok(erased)
}

fn transaction_network<S, P, B>(transaction: &Transaction<S, P, B, InMemoryDB>) -> &str
where
    S: midnight_ledger::structure::SignatureKind<InMemoryDB>,
    P: midnight_ledger::structure::ProofKind<InMemoryDB>,
    B: midnight_storage::Storable<InMemoryDB>,
{
    match transaction {
        Transaction::Standard(standard) => standard.network_id.as_str(),
        Transaction::ClaimRewards(rewards) => rewards.network_id.as_str(),
    }
}

pub(crate) fn advance_dust_balance(
    original_raw: &[u8],
    original_sealed: bool,
    balancing_raw: &[u8],
    expected_network_id: &str,
    responses: &RemoteProofResponses,
) -> Result<BalanceProgress, MidnightRuntimeError> {
    if original_raw.is_empty()
        || original_raw.len() > MAX_TRANSACTION_BYTES
        || balancing_raw.is_empty()
        || balancing_raw.len() > MAX_TRANSACTION_BYTES
        || expected_network_id.is_empty()
    {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let balancing: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&mut &balancing_raw[..])
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if transaction_network(&balancing) != expected_network_id {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut canonical_balancing = Vec::new();
    tagged_serialize(&balancing, &mut canonical_balancing)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if canonical_balancing != balancing_raw {
        return Err(MidnightRuntimeError::InvalidArgument);
    }

    let captured = Arc::new(Mutex::new(BTreeMap::new()));
    let provider = PausingProver {
        responses: Arc::new(responses.clone()),
        captured: Arc::clone(&captured),
    };
    let proven = block_on(balancing.prove(provider, &INITIAL_COST_MODEL));
    match proven {
        Ok(proven) => {
            let finalized = if original_sealed {
                let original: Transaction<
                    Signature,
                    ProofMarker,
                    PureGeneratorPedersen,
                    InMemoryDB,
                > = tagged_deserialize(&mut &original_raw[..])
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                original
                    .merge(&proven.seal(OsRng))
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?
            } else {
                let original: Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB> =
                    tagged_deserialize(&mut &original_raw[..])
                        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                original
                    .merge(&proven)
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?
                    .seal(OsRng)
            };
            let mut raw = Vec::new();
            tagged_serialize(&finalized, &mut raw)
                .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            Ok(BalanceProgress::Complete(validate_finalized_transaction(
                &raw,
                expected_network_id,
            )?))
        }
        Err(_) => {
            let request = captured
                .lock()
                .map_err(|_| MidnightRuntimeError::NativeInternal)?
                .values()
                .next()
                .cloned()
                .ok_or(MidnightRuntimeError::ProofFailed)?;
            Ok(BalanceProgress::Network(request))
        }
    }
}

pub(crate) fn finalize_balance_original(
    original_raw: &[u8],
    original_sealed: bool,
    expected_network_id: &str,
) -> Result<FinalizedTransaction, MidnightRuntimeError> {
    if original_sealed {
        return validate_finalized_transaction(original_raw, expected_network_id);
    }
    let original: Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&mut &original_raw[..])
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if transaction_network(&original) != expected_network_id {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut canonical = Vec::new();
    tagged_serialize(&original, &mut canonical)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if canonical != original_raw {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let finalized = original.seal(OsRng);
    let mut raw = Vec::new();
    tagged_serialize(&finalized, &mut raw).map_err(|_| MidnightRuntimeError::NativeInternal)?;
    validate_finalized_transaction(&raw, expected_network_id)
}

pub(crate) fn validate_unproven_transaction(
    raw: &[u8],
    expected_network_id: &str,
) -> Result<Vec<String>, MidnightRuntimeError> {
    if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES || expected_network_id.is_empty() {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let transaction: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let network_id = match &transaction {
        Transaction::Standard(standard) => standard.network_id.as_str(),
        Transaction::ClaimRewards(rewards) => rewards.network_id.as_str(),
    };
    if network_id != expected_network_id {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut canonical = Vec::new();
    tagged_serialize(&transaction, &mut canonical)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if canonical != raw {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    transaction
        .identifiers()
        .map(|identifier| super::serializable_hex(&identifier))
        .collect()
}

fn canonicalize<T>(raw: &[u8]) -> Result<Vec<u8>, MidnightRuntimeError>
where
    T: Deserializable + Serializable + Tagged,
{
    if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let transaction: T =
        tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let mut output = Vec::new();
    tagged_serialize(&transaction, &mut output)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    Ok(output)
}

pub(crate) fn canonicalize_transaction(
    signature: &str,
    proof: &str,
    binding: &str,
    raw: &[u8],
) -> Result<Vec<u8>, MidnightRuntimeError> {
    match (signature, proof, binding) {
        ("signature", "pre-proof", "pre-binding") => canonicalize::<
            Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>,
        >(raw),
        ("signature", "pre-proof", "binding") => canonicalize::<
            Transaction<Signature, ProofPreimageMarker, PureGeneratorPedersen, InMemoryDB>,
        >(raw),
        ("signature-erased", "pre-proof", "pre-binding") => canonicalize::<
            Transaction<(), ProofPreimageMarker, PedersenRandomness, InMemoryDB>,
        >(raw),
        ("signature-erased", "pre-proof", "binding") => canonicalize::<
            Transaction<(), ProofPreimageMarker, PureGeneratorPedersen, InMemoryDB>,
        >(raw),
        ("signature", "proof", "pre-binding") => {
            canonicalize::<Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB>>(raw)
        }
        ("signature", "proof", "binding") => canonicalize::<
            Transaction<Signature, ProofMarker, PureGeneratorPedersen, InMemoryDB>,
        >(raw),
        ("signature-erased", "proof", "pre-binding") => {
            canonicalize::<Transaction<(), ProofMarker, PedersenRandomness, InMemoryDB>>(raw)
        }
        ("signature-erased", "proof", "binding") => {
            canonicalize::<Transaction<(), ProofMarker, PureGeneratorPedersen, InMemoryDB>>(raw)
        }
        ("signature", "no-proof", "no-binding") => {
            canonicalize::<Transaction<Signature, (), Pedersen, InMemoryDB>>(raw)
        }
        ("signature-erased", "no-proof", "no-binding") => {
            canonicalize::<Transaction<(), (), Pedersen, InMemoryDB>>(raw)
        }
        _ => Err(MidnightRuntimeError::InvalidArgument),
    }
}

pub(crate) fn validate_finalized_transaction(
    raw: &[u8],
    expected_network_id: &str,
) -> Result<FinalizedTransaction, MidnightRuntimeError> {
    if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES || expected_network_id.is_empty() {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let transaction: Transaction<Signature, ProofMarker, PureGeneratorPedersen, InMemoryDB> =
        tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let network_id = match &transaction {
        Transaction::Standard(standard) => standard.network_id.as_str(),
        Transaction::ClaimRewards(rewards) => rewards.network_id.as_str(),
    };
    if network_id != expected_network_id {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut canonical = Vec::new();
    tagged_serialize(&transaction, &mut canonical)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    if canonical != raw {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let transaction_hash = super::serializable_hex(&transaction.transaction_hash())?;
    let identifiers = transaction
        .identifiers()
        .map(|identifier| super::serializable_hex(&identifier))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FinalizedTransaction {
        canonical,
        transaction_hash,
        identifiers,
    })
}
