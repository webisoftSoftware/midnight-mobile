//! Deterministic proof requests for the device probes and the size benchmark.
//!
//! Each request is built from a fixed seed, so every run proves the same
//! statement and timings are comparable between runs and devices.

use std::sync::Arc;

use midnight_coin_structure::coin::{Info as CoinInfo, QualifiedInfo as QualifiedCoinInfo};
use midnight_ledger::structure::ProofPreimageVersioned;
use midnight_storage::db::InMemoryDB;
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::merkle_tree::MerkleTree;
use midnight_transient_crypto::proofs::{ProvingKeyMaterial, WrappedIr};
use midnight_zswap::{Input, Output};
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::{LocalProverError, serialize_response};

fn deterministic_zswap_spend_preimage()
-> Result<Arc<midnight_transient_crypto::proofs::ProofPreimage>, LocalProverError> {
    let mut rng = StdRng::seed_from_u64(0x42);
    let qualified_coin = QualifiedCoinInfo {
        value: Default::default(),
        type_: Default::default(),
        nonce: rand::Rng::r#gen(&mut rng),
        mt_index: 0,
    };
    let coin = CoinInfo::from(&qualified_coin);
    let recipient = midnight_coin_structure::transfer::Recipient::Contract(Default::default());
    let tree = MerkleTree::<(), InMemoryDB>::blank(32)
        .try_update_hash(0, coin.commitment(&recipient).0, ())
        .map_err(|_| LocalProverError::ProofFailed)?
        .rehash();
    Input::new_contract_owned(&mut rng, &qualified_coin, None, Default::default(), &tree)
        .map(|input| input.proof)
        .map_err(|_| LocalProverError::ProofFailed)
}

fn deterministic_zswap_output_preimage()
-> Result<Arc<midnight_transient_crypto::proofs::ProofPreimage>, LocalProverError> {
    let mut rng = StdRng::seed_from_u64(0x42);
    let qualified_coin = QualifiedCoinInfo {
        value: Default::default(),
        type_: Default::default(),
        nonce: rand::Rng::r#gen(&mut rng),
        mt_index: 0,
    };
    let coin = CoinInfo::from(&qualified_coin);
    Output::<_, InMemoryDB>::new_contract_owned(&mut rng, &coin, None, Default::default())
        .map(|output| output.proof)
        .map_err(|_| LocalProverError::ProofFailed)
}

pub fn deterministic_zswap_spend_request() -> Result<Vec<u8>, LocalProverError> {
    serialize_response(&(
        ProofPreimageVersioned::V2(deterministic_zswap_spend_preimage()?),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    ))
}

pub fn deterministic_zswap_output_request() -> Result<Vec<u8>, LocalProverError> {
    serialize_response(&(
        ProofPreimageVersioned::V2(deterministic_zswap_output_preimage()?),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    ))
}

pub fn deterministic_zswap_spend_check_request() -> Result<Vec<u8>, LocalProverError> {
    serialize_response(&(
        ProofPreimageVersioned::V2(deterministic_zswap_spend_preimage()?),
        None::<WrappedIr>,
    ))
}
