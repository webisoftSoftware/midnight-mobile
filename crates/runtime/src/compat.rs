//! Ledger 8 / Ledger 9 API shims. Ledger 9 wraps signing types in a scheme enum.

use midnight_base_crypto::schnorr::SigningKey;
use rand::{CryptoRng, Rng};

#[cfg(not(feature = "ledger-v9"))]
pub use midnight_base_crypto::schnorr::{Signature, VerifyingKey as OwnerKey};
#[cfg(feature = "ledger-v9")]
pub use midnight_ledger::structure::{Signature, SignatureVerifyingKey as OwnerKey};

/// The wallet key in the form ledger structures store as an owner.
pub fn owner_key(key: &SigningKey) -> OwnerKey {
    #[cfg(feature = "ledger-v9")]
    return OwnerKey::Schnorr(key.verifying_key());
    #[cfg(not(feature = "ledger-v9"))]
    key.verifying_key()
}

/// Sign a transaction segment in the ledger's signature type.
pub fn sign(key: &SigningKey, rng: &mut (impl Rng + CryptoRng), msg: &[u8]) -> Signature {
    #[cfg(feature = "ledger-v9")]
    return Signature::Schnorr(key.sign(rng, msg));
    #[cfg(not(feature = "ledger-v9"))]
    key.sign(rng, msg)
}

/// Check a transaction signature against the wallet key.
#[cfg(test)]
pub fn verify(key: &SigningKey, msg: &[u8], signature: &Signature) -> bool {
    #[cfg(feature = "ledger-v9")]
    return <Signature as midnight_ledger::structure::SignatureKind<
        midnight_storage::db::InMemoryDB,
    >>::signature_verify::<()>(msg, owner_key(key), signature);
    #[cfg(not(feature = "ledger-v9"))]
    key.verifying_key().verify(msg, signature)
}
