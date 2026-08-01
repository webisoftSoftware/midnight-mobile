use std::borrow::Cow;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use bech32::primitives::decode::CheckedHrpstring;
use bech32::{Bech32m, Hrp};
use midnight_base_crypto::hash::HashOutput;
use midnight_base_crypto::schnorr::{Signature, SigningKey};
use midnight_coin_structure::coin::{
    Info as ShieldedCoinInfo, Nullifier as ShieldedNullifier, PublicKey as ShieldedCoinPublicKey,
    ShieldedTokenType, TokenType, UnshieldedTokenType, UserAddress,
};
use midnight_coin_structure::transfer::{Recipient, SenderEvidence};
use midnight_ledger::dust::{
    DustActions, DustGenerationInfo, DustLocalState, DustParameters, DustPublicKey,
    DustRegistration, DustSecretKey, INITIAL_DUST_PARAMETERS, QualifiedDustOutput, successor_utxo,
};
use midnight_ledger::events::Event;
use midnight_ledger::semantics::ZswapLocalStateExt;
use midnight_ledger::structure::{
    Intent, IntentHash, LedgerParameters, ProofPreimageMarker, Transaction, UnshieldedOffer,
    UtxoOutput, UtxoSpend,
};
use midnight_serialize::{
    Deserializable, Serializable, Tagged, tagged_deserialize, tagged_deserialize_sequence,
    tagged_serialize,
};
use midnight_storage::arena::Sp;
use midnight_storage::db::InMemoryDB;
use midnight_storage::storage::HashMap as LedgerHashMap;
use midnight_transient_crypto::merkle_tree::{MerkleTree, MerkleTreeCollapsedUpdate};
use midnight_transient_crypto::proofs::ProofPreimage;
use midnight_transient_crypto::{
    commitment::{Pedersen, PedersenRandomness},
    encryption,
};
use midnight_zswap::keys::{SecretKeys as ZswapSecretKeys, Seed as ZswapSeed};
use midnight_zswap::{Offer as ZswapOffer, Output as ZswapOutput};
use num_bigint::BigUint;
use rand::rngs::OsRng;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::{MidnightRuntimeError, WalletAddressMaterial, serializable_hex};

mod dapp;
pub(crate) use dapp::{DappIntentBuildInput, DappIntentInput, DappTransactionOutput};

const MAX_LEGACY_STATE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SYNC_ENTRIES: usize = 1_000_000;
const V2_SHIELDED_RECORD_HEADER_BYTES: usize = 61;
const DUST_SPEND_HEADER_BYTES: usize = 12;
const DUST_SPEND_WASM_RECORD_BYTES: usize = 36;
const DUST_SPEND_EXTENDED_RECORD_BYTES: usize = 48;
const DUST_COMMITMENT_TREE_DEPTH: u8 = 32;

include!("wallet_state/types.rs");

fn canonical_decimal(value: &str) -> bool {
    value == "0"
        || value
            .strip_prefix(|character: char| ('1'..='9').contains(&character))
            .is_some_and(|rest| rest.bytes().all(|byte| byte.is_ascii_digit()))
}

fn nonempty_bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max
}

fn decode_hex_state<T: midnight_serialize::Deserializable + midnight_serialize::Tagged>(
    value: &str,
) -> Result<T, MidnightRuntimeError> {
    if value.is_empty() || value.len() > MAX_LEGACY_STATE_BYTES.saturating_mul(2) {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    let raw = hex::decode(value).map_err(|_| MidnightRuntimeError::StateIncompatible)?;
    tagged_deserialize(&raw[..]).map_err(|_| MidnightRuntimeError::StateIncompatible)
}

fn encode_hex_state<T: midnight_serialize::Serializable + midnight_serialize::Tagged>(
    state: &T,
) -> Result<String, MidnightRuntimeError> {
    let mut raw = Vec::new();
    tagged_serialize(state, &mut raw).map_err(|_| MidnightRuntimeError::NativeInternal)?;
    Ok(hex::encode(raw))
}

fn parse_snapshot<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, MidnightRuntimeError> {
    if raw.is_empty() || raw.len() > MAX_LEGACY_STATE_BYTES {
        return Err(MidnightRuntimeError::StateIncompatible);
    }
    serde_json::from_str(raw).map_err(|_| MidnightRuntimeError::StateIncompatible)
}

fn to_snapshot_json<T: Serialize>(value: &T) -> Result<String, MidnightRuntimeError> {
    serde_json::to_string(value).map_err(|_| MidnightRuntimeError::NativeInternal)
}

#[derive(Clone, Debug)]
pub(crate) struct NativeWalletState {
    shielded: midnight_zswap::local::State<InMemoryDB>,
    dust: DustLocalState<InMemoryDB>,
    unshielded: UnshieldedCollections,
    coin_hashes: BTreeMap<String, CoinHashes>,
    protocol_version: String,
}

pub(crate) struct ShieldedMintContext {
    pub(crate) coin_public_key: ShieldedCoinPublicKey,
    pub(crate) encryption_public_key: encryption::PublicKey,
    pub(crate) output_index: u64,
}

pub(crate) struct RestoreContext<'a> {
    pub network_id: &'a str,
    pub unshielded_address: &'a str,
    pub address_material: &'a WalletAddressMaterial,
    pub night_external_key: &'a [u8],
}

pub(crate) struct LegacyExportContext<'a> {
    pub network_id: &'a str,
    pub unshielded_address: &'a str,
    pub address_material: &'a WalletAddressMaterial,
    pub night_external_key: &'a [u8],
    pub shielded_offset: Option<u64>,
    pub dust_offset: Option<u64>,
    pub unshielded_offset: Option<u64>,
}

pub(crate) struct DustBalanceInput<'a> {
    pub network_id: &'a str,
    pub original: &'a Transaction<Signature, (), Pedersen, InMemoryDB>,
    pub ledger_parameters: &'a LedgerParameters,
    pub fee_blocks_margin: usize,
    pub additional_fee_overhead: u128,
    pub dust_seed: &'a [u8],
    pub current_time_seconds: u64,
    pub ttl_seconds: u64,
}

struct UnshieldedTransferInput<'a> {
    network_id: &'a str,
    target_address: &'a str,
    amount: u128,
    token_type: &'a str,
    night_external_key: &'a [u8],
    ttl_seconds: u64,
}

type DustBalancingResult = Result<
    (
        DustLocalState<InMemoryDB>,
        Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>,
    ),
    MidnightRuntimeError,
>;

include!("wallet_state/sync.rs");
include!("wallet_state/restore.rs");
include!("wallet_state/apply_and_prepare.rs");
include!("wallet_state/spend_builders.rs");
include!("wallet_state/shielded_and_state.rs");

#[cfg(test)]
mod tests;

fn decode_hash(value: &str) -> Result<HashOutput, MidnightRuntimeError> {
    let bytes = hex::decode(value).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    Ok(HashOutput(bytes))
}

fn decode_shielded_address(
    value: &str,
    network_id: &str,
) -> Result<(ShieldedCoinPublicKey, encryption::PublicKey), MidnightRuntimeError> {
    let parsed = CheckedHrpstring::new::<Bech32m>(value)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let expected_hrp = if network_id == "mainnet" {
        "mn_shield-addr".to_owned()
    } else {
        format!("mn_shield-addr_{network_id}")
    };
    let expected_hrp =
        Hrp::parse(&expected_hrp).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if parsed.hrp() != expected_hrp {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let bytes = parsed.byte_iter().collect::<Vec<_>>();
    if bytes.len() != 64 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let cpk = ShieldedCoinPublicKey(decode_hash(&hex::encode(&bytes[..32]))?);
    let mut epk_bytes = &bytes[32..];
    let epk = <encryption::PublicKey as Deserializable>::deserialize(&mut epk_bytes, 0)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if !epk_bytes.is_empty() {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok((cpk, epk))
}

fn decode_unshielded_address(
    value: &str,
    network_id: &str,
) -> Result<UserAddress, MidnightRuntimeError> {
    let parsed = CheckedHrpstring::new::<Bech32m>(value)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let expected_hrp = if network_id == "mainnet" {
        "mn_addr".to_owned()
    } else {
        format!("mn_addr_{network_id}")
    };
    let expected_hrp =
        Hrp::parse(&expected_hrp).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if parsed.hrp() != expected_hrp {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let bytes = parsed.byte_iter().collect::<Vec<_>>();
    let mut bytes = &bytes[..];
    let address = <UserAddress as Deserializable>::deserialize(&mut bytes, 0)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if !bytes.is_empty() {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok(address)
}

fn add_balance(balances: &mut BTreeMap<String, u128>, token_type: String, value: u128) {
    let current = balances.get(&token_type).copied().unwrap_or(0);
    balances.insert(token_type, current.saturating_add(value));
}

fn decimal_balances(values: BTreeMap<String, u128>) -> BTreeMap<String, String> {
    values
        .into_iter()
        .map(|(token, value)| (token, value.to_string()))
        .collect()
}

fn decode_event_payloads(
    payloads: &[Vec<u8>],
) -> Result<Vec<Event<InMemoryDB>>, MidnightRuntimeError> {
    let mut events = Vec::new();
    for payload in payloads {
        if payload.is_empty() {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        events.extend(
            tagged_deserialize_sequence(&payload[..])
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
        );
    }
    Ok(events)
}

fn same_utxo(left: &UnshieldedUtxo, right: &UnshieldedUtxo) -> bool {
    left.intent_hash == right.intent_hash && left.output_no == right.output_no
}

fn remove_utxo(values: &mut Vec<UnshieldedUtxoWithMeta>, target: &UnshieldedUtxo) {
    values.retain(|value| !same_utxo(&value.utxo, target));
}

fn upsert_utxo(values: &mut Vec<UnshieldedUtxoWithMeta>, value: UnshieldedUtxoWithMeta) {
    remove_utxo(values, &value.utxo);
    values.push(value);
}
