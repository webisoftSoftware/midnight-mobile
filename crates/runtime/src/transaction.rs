use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

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
use super::executor::block_on;

mod remote_proof;

use remote_proof::PausingProver;
pub(crate) use remote_proof::{
    RemoteProofKeyMaterials, RemoteProofKind, RemoteProofRequest, RemoteProofResponses,
};

#[cfg(test)]
mod tests;

const MAX_TRANSACTION_BYTES: usize = 64 * 1024 * 1024;
/// Upper bound on the proof requests handed out in one round. Emitting a subset is
/// always correct — the next replay re-derives whatever was left — so this caps peak
/// live secret bodies and concurrent prover work without affecting the outcome.
pub(crate) const MAX_PROOF_BATCH: usize = 64;

pub(crate) struct FinalizedTransaction {
    pub canonical: Vec<u8>,
    pub transaction_hash: String,
    pub identifiers: Vec<String>,
}

pub(crate) enum BalanceProgress {
    Network(Vec<RemoteProofRequest>),
    Complete(FinalizedTransaction),
}

/// Drains every request the paused prove tree captured in this poll.
///
/// The upstream prover fans out under `futures::join!`/`join_all` and none of our leaf
/// futures yield on real I/O, so a single poll reaches every branch whose preimage does
/// not depend on an earlier proof's result. Iteration follows the `BTreeMap`'s
/// body-hash order, which makes the batch reproducible for a given transaction.
fn captured_batch(
    captured: &Mutex<BTreeMap<String, RemoteProofRequest>>,
) -> Result<Vec<RemoteProofRequest>, MidnightRuntimeError> {
    let requests = captured
        .lock()
        .map_err(|_| MidnightRuntimeError::NativeInternal)?
        .values()
        .take(MAX_PROOF_BATCH)
        .cloned()
        .collect::<Vec<_>>();
    if requests.is_empty() {
        return Err(MidnightRuntimeError::ProofFailed);
    }
    Ok(requests)
}

pub(crate) fn advance_unproven_transaction(
    raw: &[u8],
    expected_network_id: &str,
    responses: &RemoteProofResponses,
) -> Result<BalanceProgress, MidnightRuntimeError> {
    advance_unproven_transaction_with_materials(
        raw,
        expected_network_id,
        responses,
        &RemoteProofKeyMaterials::default(),
    )
}

pub(crate) fn advance_unproven_transaction_with_materials(
    raw: &[u8],
    expected_network_id: &str,
    responses: &RemoteProofResponses,
    key_material: &RemoteProofKeyMaterials,
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
    key_material.validate_locations(&proof_key_locations(&transaction))?;

    let captured = Arc::new(Mutex::new(BTreeMap::new()));
    let provider = PausingProver {
        responses,
        key_material,
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
        Err(_) => Ok(BalanceProgress::Network(captured_batch(&captured)?)),
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
    let key_material = RemoteProofKeyMaterials::default();
    let provider = PausingProver {
        responses,
        key_material: &key_material,
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
        Err(_) => Ok(BalanceProgress::Network(captured_batch(&captured)?)),
    }
}

fn proof_key_locations(
    transaction: &Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>,
) -> BTreeSet<String> {
    let mut locations = BTreeSet::new();
    let Transaction::Standard(standard) = transaction else {
        return locations;
    };
    for input in standard.inputs() {
        locations.insert(input.proof.key_location.0.to_string());
    }
    for output in standard.outputs() {
        locations.insert(output.proof.key_location.0.to_string());
    }
    for (_, call) in standard.calls() {
        locations.insert(call.proof.key_location().0.to_string());
    }
    for (_, intent) in standard.intents() {
        if let Some(actions) = intent.dust_actions.as_ref() {
            for spend in actions.spends.iter_deref() {
                locations.insert(spend.proof.key_location.0.to_string());
            }
        }
    }
    locations
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
