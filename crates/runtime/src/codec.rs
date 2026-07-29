use std::{io::Read, sync::Arc};

use flate2::read::GzDecoder;

use midnight_ledger::structure::ProofPreimageVersioned;
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::proofs::{ProofPreimage, ProvingKeyMaterial, WrappedIr};
use num_bigint::BigUint;
use zeroize::Zeroize;

use super::MidnightRuntimeError;

const MAX_CODEC_INPUT_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn decompress_gzip(serialized: &[u8]) -> Result<Vec<u8>, MidnightRuntimeError> {
    if serialized.is_empty() || serialized.len() > MAX_CODEC_INPUT_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let decoder = GzDecoder::new(serialized);
    let mut output = Vec::new();
    decoder
        .take((MAX_CODEC_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if output.is_empty() || output.len() > MAX_CODEC_INPUT_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok(output)
}

fn decode_preimage(serialized: &[u8]) -> Result<ProofPreimageVersioned, MidnightRuntimeError> {
    if serialized.is_empty() || serialized.len() > MAX_CODEC_INPUT_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    tagged_deserialize(&mut &serialized[..])
        .or_else(|_| {
            tagged_deserialize::<Arc<ProofPreimage>>(&mut &serialized[..])
                .map(ProofPreimageVersioned::V2)
        })
        .map_err(|_| MidnightRuntimeError::InvalidArgument)
}

fn serialize_payload<T: midnight_serialize::Serializable + midnight_serialize::Tagged>(
    payload: &T,
) -> Result<Vec<u8>, MidnightRuntimeError> {
    let mut output = Vec::new();
    tagged_serialize(payload, &mut output).map_err(|_| MidnightRuntimeError::NativeInternal)?;
    Ok(output)
}

pub(crate) fn parse_check_result(
    serialized_result: &[u8],
) -> Result<Vec<Option<u64>>, MidnightRuntimeError> {
    if serialized_result.is_empty() || serialized_result.len() > MAX_CODEC_INPUT_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    tagged_deserialize(&mut &serialized_result[..])
        .map_err(|_| MidnightRuntimeError::InvalidArgument)
}

fn binding_input_from_decimal(value: &str) -> Result<Fr, MidnightRuntimeError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let value =
        BigUint::parse_bytes(value.as_bytes(), 10).ok_or(MidnightRuntimeError::InvalidArgument)?;
    Fr::from_le_bytes(&value.to_bytes_le()).ok_or(MidnightRuntimeError::InvalidArgument)
}

pub(crate) fn create_check_payload(
    serialized_preimage: &[u8],
    mut ir: Option<Vec<u8>>,
) -> Result<Vec<u8>, MidnightRuntimeError> {
    let result = (|| {
        if ir
            .as_ref()
            .is_some_and(|value| value.len() > MAX_CODEC_INPUT_BYTES)
        {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let preimage = decode_preimage(serialized_preimage)?;
        let mut payload = (preimage, ir.take().map(WrappedIr));
        let result = serialize_payload(&payload);
        if let Some(ir) = &mut payload.1 {
            ir.0.zeroize();
        }
        result
    })();
    if let Some(ir) = &mut ir {
        ir.zeroize();
    }
    result
}

pub(crate) fn create_proving_payload(
    serialized_preimage: &[u8],
    binding_input: Option<&str>,
    mut key_material: Option<ProvingKeyMaterial>,
) -> Result<Vec<u8>, MidnightRuntimeError> {
    let result = (|| {
        if key_material.as_ref().is_some_and(|material| {
            material.prover_key.len() > MAX_CODEC_INPUT_BYTES
                || material.verifier_key.len() > MAX_CODEC_INPUT_BYTES
                || material.ir_source.len() > MAX_CODEC_INPUT_BYTES
        }) {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let preimage = decode_preimage(serialized_preimage)?;
        let binding_input = binding_input.map(binding_input_from_decimal).transpose()?;
        let mut payload = (preimage, key_material.take(), binding_input);
        let result = serialize_payload(&payload);
        if let Some(material) = &mut payload.1 {
            material.ir_source.zeroize();
            material.prover_key.zeroize();
            material.verifier_key.zeroize();
        }
        result
    })();
    if let Some(material) = &mut key_material {
        material.ir_source.zeroize();
        material.prover_key.zeroize();
        material.verifier_key.zeroize();
    }
    result
}

#[cfg(test)]
#[cfg(test)]
fn synthetic_preimage_bytes() -> Vec<u8> {
    use std::borrow::Cow;

    use midnight_transient_crypto::proofs::KeyLocation;

    let preimage = ProofPreimage {
        inputs: vec![1_u64.into(), 2_u64.into()],
        private_transcript: vec![3_u64.into()],
        public_transcript_inputs: vec![4_u64.into()],
        public_transcript_outputs: vec![5_u64.into()],
        binding_input: 6_u64.into(),
        communications_commitment: None,
        key_location: KeyLocation(Cow::Borrowed("native/fixture")),
    };
    serialize_payload(&ProofPreimageVersioned::V2(Arc::new(preimage))).unwrap()
}

#[cfg(test)]
mod tests {
    use midnight_serialize::tagged_deserialize;

    use super::*;

    #[test]
    fn synthetic_preimage_round_trips_through_check_payload() {
        let payload = create_check_payload(&synthetic_preimage_bytes(), Some(vec![7, 8])).unwrap();
        let decoded: (ProofPreimageVersioned, Option<WrappedIr>) =
            tagged_deserialize(&mut &payload[..]).unwrap();
        assert!(matches!(decoded.0, ProofPreimageVersioned::V2(_)));
        assert_eq!(decoded.1.unwrap().0, vec![7, 8]);
    }

    #[test]
    fn malformed_codec_inputs_are_rejected() {
        assert!(create_check_payload(b"invalid", None).is_err());
        assert!(parse_check_result(b"invalid").is_err());
    }

    #[test]
    fn check_results_preserve_u64_values() {
        let expected = vec![Some(0), None, Some(u64::MAX)];
        let encoded = serialize_payload(&expected).unwrap();
        assert_eq!(parse_check_result(&encoded).unwrap(), expected);
    }
}
