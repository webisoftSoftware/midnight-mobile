//! ZKIR v3 for native provers that still use Ledger 8 request types.

use std::io;
use std::io::Cursor;

use midnight_serialize_v3::tagged_deserialize;
use midnight_transient_crypto::proofs::{
    Proof as RawProof, ProofPreimage as V2Preimage, ProvingKeyMaterial as V2Material,
};
use midnight_transient_crypto_v3::proofs::{
    KeyLocation, ParamsProverProvider, ProofPreimage, ProvingKeyMaterial, Resolver, VerifierKey,
    Zkir,
};
use midnight_zkir_v3::IrSource;
use rand::rngs::OsRng;
use zeroize::Zeroizing;

pub(super) use midnight_transient_crypto_v3::proofs::ParamsProver;

pub(super) fn matches_tag(ir: &[u8]) -> bool {
    ir.starts_with(b"midnight:ir-source[v3-generic]:")
}

fn decode_preimage(preimage: &V2Preimage) -> io::Result<ProofPreimage> {
    let mut bytes = Zeroizing::new(Vec::new());
    midnight_serialize::tagged_serialize(preimage, &mut *bytes)?;
    tagged_deserialize(&bytes[..])
}

pub(super) fn circuit_size(ir: &[u8]) -> io::Result<u8> {
    Ok(tagged_deserialize::<IrSource>(ir)?.k())
}

pub(super) fn validate_keys(material: &V2Material) -> io::Result<u8> {
    let k = circuit_size(&material.ir_source)?;
    let key = IrSource::load_prover_key_from_tagged(Cursor::new(&material.prover_key))?;
    if key.init().map_err(io::Error::other)?.k() != k {
        return Err(io::Error::other("prover key and circuit sizes differ"));
    }
    let key: VerifierKey = tagged_deserialize(&material.verifier_key[..])?;
    key.init().map_err(io::Error::other)?;
    Ok(k)
}

pub(super) fn check(preimage: &V2Preimage, ir: &[u8]) -> io::Result<Vec<Option<usize>>> {
    let ir: IrSource = tagged_deserialize(ir)?;
    decode_preimage(preimage)?
        .check(&ir)
        .map_err(io::Error::other)
}

struct Provider<'a, F> {
    material: &'a V2Material,
    params: F,
}

impl<F: Fn(u8) -> io::Result<ParamsProver>> ParamsProverProvider for Provider<'_, F> {
    async fn get_params(&self, k: u8) -> io::Result<ParamsProver> {
        (self.params)(k)
    }
}

impl<F> Resolver for Provider<'_, F> {
    async fn resolve_key(&self, _key: KeyLocation) -> io::Result<Option<ProvingKeyMaterial>> {
        Ok(Some(ProvingKeyMaterial {
            prover_key: self.material.prover_key.clone(),
            verifier_key: self.material.verifier_key.clone(),
            ir_source: self.material.ir_source.clone(),
        }))
    }
}

pub(super) async fn prove(
    preimage: &V2Preimage,
    material: &V2Material,
    params: impl Fn(u8) -> io::Result<ParamsProver>,
) -> io::Result<RawProof> {
    let provider = Provider { material, params };
    let (proof, _) = decode_preimage(preimage)?
        .prove::<IrSource>(OsRng, &provider, &provider)
        .await
        .map_err(io::Error::other)?;
    Ok(RawProof(proof.0))
}

#[cfg(test)]
mod tests {
    use super::matches_tag;

    #[test]
    fn matches_only_the_full_v3_header() {
        assert!(matches_tag(b"midnight:ir-source[v3-generic]:\x01"));
        assert!(!matches_tag(b"midnight:ir-source[v2-generic]:\x01"));
        assert!(!matches_tag(b"midnight:ir-source[v3-generic]"));
        assert!(!matches_tag(b"xmidnight:ir-source[v3-generic]:"));
    }
}
