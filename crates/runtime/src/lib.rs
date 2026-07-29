use midnight_base_crypto::schnorr::SigningKey;
use midnight_coin_structure::coin::UserAddress;
use midnight_ledger::dust::{DustPublicKey, DustSecretKey};
use midnight_serialize::{Serializable, tagged_deserialize};
use midnight_zswap::keys::{SecretKeys as ZswapSecretKeys, Seed as ZswapSeed};
use num_bigint::BigUint;
use zeroize::Zeroize;

mod codec;
mod runtime;
mod transaction;
mod wallet_state;

pub use runtime::{
    RuntimeSessionHandle, apply_sync_batch, begin_command, cancel_operation, close_wallet_session,
    export_wallet_checkpoint, get_wallet_snapshot, open_wallet_session, resume_operation,
};

const SOURCE_REVISION: &str = "02716c2c95d50654aeb3cb63bfd8386046e4ca7d";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAddressMaterial {
    pub unshielded_address_hex: String,
    pub shielded_coin_public_key_hex: String,
    pub shielded_encryption_public_key_hex: String,
    pub dust_public_key: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MidnightRuntimeError {
    #[error("UNAVAILABLE")]
    Unavailable,
    #[error("INVALID_ARGUMENT")]
    InvalidArgument,
    #[error("STALE_SESSION")]
    StaleSession,
    #[error("CANCELLED")]
    Cancelled,
    #[error("STATE_INCOMPATIBLE")]
    StateIncompatible,
    #[error("SYNC_GAP")]
    SyncGap,
    #[error("PROOF_FAILED")]
    ProofFailed,
    #[error("INSUFFICIENT_DUST")]
    InsufficientDust,
    #[error("SUBMISSION_STATUS_UNKNOWN")]
    SubmissionStatusUnknown,
    #[error("INVALID_LENGTH")]
    InvalidLength,
    #[error("DECODE_FAILED")]
    DecodeFailed,
    #[error("NATIVE_INTERNAL")]
    NativeInternal,
}

pub fn parse_check_result(data: Vec<u8>) -> Result<Vec<Option<String>>, MidnightRuntimeError> {
    let values: Vec<Option<u64>> =
        tagged_deserialize(&data[..]).map_err(|_| MidnightRuntimeError::DecodeFailed)?;
    Ok(values
        .into_iter()
        .map(|value| value.map(|integer| integer.to_string()))
        .collect())
}

pub fn derive_wallet_address_material(
    mut night_external_key: Vec<u8>,
    mut zswap_seed: Vec<u8>,
    mut dust_seed: Vec<u8>,
) -> Result<WalletAddressMaterial, MidnightRuntimeError> {
    let result = derive_wallet_address_material_inner(&night_external_key, &zswap_seed, &dust_seed);

    // UniFFI owns these buffers after lifting them across the FFI boundary.
    // Clear every secret input on every success or error path before returning.
    night_external_key.zeroize();
    zswap_seed.zeroize();
    dust_seed.zeroize();
    result
}

fn derive_wallet_address_material_inner(
    night_external_key: &[u8],
    zswap_seed: &[u8],
    dust_seed: &[u8],
) -> Result<WalletAddressMaterial, MidnightRuntimeError> {
    // Validate the complete request before deriving any address family so the
    // production operation remains all-or-nothing.
    if night_external_key.len() != 32 || zswap_seed.len() != 32 || dust_seed.len() != 32 {
        return Err(MidnightRuntimeError::InvalidLength);
    }

    let signing_key = SigningKey::from_bytes(night_external_key)
        .map_err(|_| MidnightRuntimeError::DecodeFailed)?;
    let verifying_key = signing_key.verifying_key();
    let user_address = UserAddress::from(verifying_key);

    let mut zswap_seed_bytes: [u8; 32] = zswap_seed
        .try_into()
        .map_err(|_| MidnightRuntimeError::InvalidLength)?;
    let zswap_keys = ZswapSecretKeys::from(ZswapSeed::from(zswap_seed_bytes));
    zswap_seed_bytes.zeroize();

    let mut dust_seed_bytes: [u8; 32] = dust_seed
        .try_into()
        .map_err(|_| MidnightRuntimeError::InvalidLength)?;
    let dust_secret_key = DustSecretKey::derive_secret_key(&dust_seed_bytes);
    dust_seed_bytes.zeroize();
    let dust_public_key = DustPublicKey::from(dust_secret_key);

    Ok(WalletAddressMaterial {
        unshielded_address_hex: serializable_hex(&user_address)?,
        shielded_coin_public_key_hex: serializable_hex(&zswap_keys.coin_public_key())?,
        shielded_encryption_public_key_hex: serializable_hex(&zswap_keys.enc_public_key())?,
        dust_public_key: BigUint::from_bytes_le(&dust_public_key.0.as_le_bytes()).to_str_radix(10),
    })
}

fn serializable_hex<T: Serializable + ?Sized>(value: &T) -> Result<String, MidnightRuntimeError> {
    let mut serialized = Vec::new();
    value
        .serialize(&mut serialized)
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
    Ok(hex::encode(serialized))
}

uniffi::setup_scaffolding!();

#[cfg(test)]
mod tests {
    use super::*;
    use midnight_serialize::tagged_serialize;

    #[test]
    fn parses_a_synthetic_check_result() {
        let expected = vec![None, Some(7_u64), Some(42_u64)];
        let mut encoded = Vec::new();
        tagged_serialize(&expected, &mut encoded).unwrap();
        assert_eq!(
            parse_check_result(encoded).unwrap(),
            vec![None, Some("7".to_owned()), Some("42".to_owned())]
        );
    }

    #[test]
    fn rejects_malformed_check_results() {
        assert_eq!(
            parse_check_result(vec![0xff]).unwrap_err().to_string(),
            "DECODE_FAILED"
        );
    }

    #[test]
    fn synthetic_derivation_is_deterministic() {
        let derive =
            || derive_wallet_address_material(vec![1; 32], vec![2; 32], vec![3; 32]).unwrap();
        assert_eq!(derive(), derive());
    }

    #[test]
    fn rejects_invalid_secret_lengths() {
        for bad_length in [31, 33] {
            for bad_input in 0..3 {
                let mut inputs = [vec![1; 32], vec![2; 32], vec![3; 32]];
                inputs[bad_input] = vec![0x5a; bad_length];
                let [night, zswap, dust] = inputs;
                let error = derive_wallet_address_material(night, zswap, dust)
                    .unwrap_err()
                    .to_string();
                assert_eq!(error, "INVALID_LENGTH");
                assert!(!error.contains("5a"));
            }
        }
    }
}
