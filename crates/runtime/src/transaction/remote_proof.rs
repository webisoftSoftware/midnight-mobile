use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use midnight_ledger::structure::ProofVersioned;
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::proofs::{Proof, ProofPreimage, ProvingProvider};
use num_bigint::BigUint;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::{MAX_TRANSACTION_BYTES, MidnightRuntimeError};
use crate::codec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoteProofKind {
    Check,
    Prove,
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteProofRequest {
    pub key: String,
    pub kind: RemoteProofKind,
    pub body: Vec<u8>,
}

impl Drop for RemoteProofRequest {
    fn drop(&mut self) {
        self.body.zeroize();
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RemoteProofResponses {
    values: HashMap<String, Vec<u8>>,
}

impl Drop for RemoteProofResponses {
    fn drop(&mut self) {
        for response in self.values.values_mut() {
            response.zeroize();
        }
    }
}

#[derive(Clone)]
pub(super) struct PausingProver {
    pub(super) responses: Arc<RemoteProofResponses>,
    pub(super) captured: Arc<Mutex<BTreeMap<String, RemoteProofRequest>>>,
}

fn remote_key(kind: RemoteProofKind, body: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update([match kind {
        RemoteProofKind::Check => 0,
        RemoteProofKind::Prove => 1,
    }]);
    digest.update(body);
    hex::encode(digest.finalize())
}

fn serialize_preimage(preimage: &ProofPreimage) -> Result<Vec<u8>, anyhow::Error> {
    let mut raw = Vec::new();
    tagged_serialize(preimage, &mut raw)?;
    Ok(raw)
}

fn decode_proof(raw: &[u8]) -> Result<Proof, anyhow::Error> {
    if let Ok(proof) = tagged_deserialize(&mut &raw[..]) {
        return Ok(proof);
    }
    let versioned: ProofVersioned = tagged_deserialize(&mut &raw[..])?;
    match versioned {
        ProofVersioned::V2(proof) => Ok(proof),
        _ => Err(anyhow::anyhow!("unsupported proof version")),
    }
}

impl PausingProver {
    fn capture(&self, request: RemoteProofRequest) -> Result<(), anyhow::Error> {
        self.captured
            .lock()
            .map_err(|_| anyhow::anyhow!("proof request state unavailable"))?
            .entry(request.key.clone())
            .or_insert(request);
        Ok(())
    }
}

impl ProvingProvider for PausingProver {
    async fn check(&self, preimage: &ProofPreimage) -> Result<Vec<Option<usize>>, anyhow::Error> {
        let mut raw = serialize_preimage(preimage)?;
        let body_result = codec::create_check_payload(&raw, None)
            .map_err(|_| anyhow::anyhow!("invalid check preimage"));
        raw.zeroize();
        let mut body = body_result?;
        let key = remote_key(RemoteProofKind::Check, &body);
        if let Some(response) = self.responses.values.get(&key) {
            let parsed = codec::parse_check_result(response)
                .map_err(|_| anyhow::anyhow!("invalid check response"))?
                .into_iter()
                .map(|value| {
                    value
                        .map(usize::try_from)
                        .transpose()
                        .map_err(|_| anyhow::anyhow!("check response out of range"))
                })
                .collect();
            body.zeroize();
            return parsed;
        }
        self.capture(RemoteProofRequest {
            key,
            kind: RemoteProofKind::Check,
            body,
        })?;
        Err(anyhow::anyhow!("remote check required"))
    }

    async fn prove(
        self,
        preimage: &ProofPreimage,
        overwrite_binding_input: Option<Fr>,
    ) -> Result<Proof, anyhow::Error> {
        let mut raw = serialize_preimage(preimage)?;
        let binding = overwrite_binding_input
            .map(|value| BigUint::from_bytes_le(&value.as_le_bytes()).to_str_radix(10));
        let body_result = codec::create_proving_payload(&raw, binding.as_deref(), None)
            .map_err(|_| anyhow::anyhow!("invalid proving preimage"));
        raw.zeroize();
        let mut body = body_result?;
        let key = remote_key(RemoteProofKind::Prove, &body);
        if let Some(response) = self.responses.values.get(&key) {
            let proof = decode_proof(response);
            body.zeroize();
            return proof;
        }
        self.capture(RemoteProofRequest {
            key,
            kind: RemoteProofKind::Prove,
            body,
        })?;
        Err(anyhow::anyhow!("remote proof required"))
    }

    fn split(&mut self) -> Self {
        self.clone()
    }
}

impl RemoteProofResponses {
    pub(crate) fn accept(
        &mut self,
        request: &RemoteProofRequest,
        response: Vec<u8>,
    ) -> Result<(), MidnightRuntimeError> {
        if response.is_empty() || response.len() > MAX_TRANSACTION_BYTES {
            return Err(MidnightRuntimeError::ProofFailed);
        }
        match request.kind {
            RemoteProofKind::Check => {
                codec::parse_check_result(&response)
                    .map_err(|_| MidnightRuntimeError::ProofFailed)?;
            }
            RemoteProofKind::Prove => {
                decode_proof(&response).map_err(|_| MidnightRuntimeError::ProofFailed)?;
            }
        }
        self.values.insert(request.key.clone(), response);
        Ok(())
    }
}
