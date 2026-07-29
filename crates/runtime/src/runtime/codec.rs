fn to_json<T: Serialize>(value: &T) -> Result<String, MidnightRuntimeError> {
    serde_json::to_string(value).map_err(|_| MidnightRuntimeError::NativeInternal)
}

fn serialize_embedded_json<S>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(value) => serde_json::from_str::<serde_json::Value>(value)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer),
        None => serializer.serialize_none(),
    }
}

pub(crate) fn decode_base64(value: &str) -> Result<Vec<u8>, MidnightRuntimeError> {
    // The Expo boundary already transports bytes as base64. Decode the narrow
    // command field here without adding a second platform-specific codec.
    let mut output = Vec::with_capacity(value.len().saturating_mul(3) / 4);
    let mut quartet = [0_u8; 4];
    let mut count = 0;
    let mut saw_padding = false;
    for byte in value.bytes() {
        let decoded = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                saw_padding = true;
                64
            }
            _ => return Err(MidnightRuntimeError::InvalidArgument),
        };
        if saw_padding && decoded != 64 {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        quartet[count] = decoded;
        count += 1;
        if count == 4 {
            if quartet[0] == 64 || quartet[1] == 64 {
                return Err(MidnightRuntimeError::InvalidArgument);
            }
            output.push((quartet[0] << 2) | (quartet[1] >> 4));
            if quartet[2] != 64 {
                output.push((quartet[1] << 4) | (quartet[2] >> 2));
            }
            if quartet[3] != 64 {
                if quartet[2] == 64 {
                    return Err(MidnightRuntimeError::InvalidArgument);
                }
                output.push((quartet[2] << 6) | quartet[3]);
            }
            count = 0;
        }
    }
    if count != 0 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok(output)
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn signing_transcript(domain: &str, data: &[u8]) -> Result<Vec<u8>, MidnightRuntimeError> {
    let domain = domain.as_bytes();
    if domain.is_empty() || domain.len() > MAX_SIGNING_DOMAIN_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    if data.len() > MAX_DAPP_SIGN_DATA_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let domain_len =
        u64::try_from(domain.len()).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let data_len = u64::try_from(data.len()).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let mut transcript = Vec::with_capacity(
        b"midnight-mobile/sign-data".len() + 1 + 8 + domain.len() + 8 + data.len(),
    );
    transcript.extend_from_slice(b"midnight-mobile/sign-data");
    transcript.push(1);
    transcript.extend_from_slice(&domain_len.to_be_bytes());
    transcript.extend_from_slice(domain);
    transcript.extend_from_slice(&data_len.to_be_bytes());
    transcript.extend_from_slice(data);
    Ok(transcript)
}

fn sign_data_locked(
    state: &SessionState,
    domain: &str,
    data: &[u8],
) -> Result<String, MidnightRuntimeError> {
    let signing_key = SigningKey::from_bytes(&state.secrets.night_external_key)
        .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
    let transcript = signing_transcript(domain, data)?;
    let signature = signing_key.sign(&mut OsRng, &transcript);
    to_json(&serde_json::json!({
        "signatureHex": serializable_hex(&signature)?,
        "verifyingKeyHex": serializable_hex(&signing_key.verifying_key())?,
    }))
}

fn complete_json(result_json: String) -> Result<String, MidnightRuntimeError> {
    to_json(&OperationStep {
        kind: "complete",
        operation: None,
        effect_id: None,
        effect: None,
        endpoint_role: None,
        body_base64: None,
        result_json: Some(result_json),
    })
}

fn finalized_transaction_result(
    finalized: &transaction::FinalizedTransaction,
) -> Result<String, MidnightRuntimeError> {
    to_json(&serde_json::json!({
        "transactionBase64": encode_base64(&finalized.canonical),
        "transactionHash": hex::encode(Sha256::digest(&finalized.canonical)),
        "ledgerTransactionHash": finalized.transaction_hash,
        "identifiers": finalized.identifiers,
    }))
}

fn balance_service_transaction_result(
    response: &BalanceServiceResult,
    finalized: &transaction::FinalizedTransaction,
    transaction_hash: &str,
) -> Result<String, MidnightRuntimeError> {
    let mut result = serde_json::json!({
        "transactionBase64": encode_base64(&finalized.canonical),
        "transactionHash": transaction_hash,
        "ledgerTransactionHash": finalized.transaction_hash,
        "identifiers": finalized.identifiers,
    });
    if let Some(expires_at) = response.expires_at {
        result
            .as_object_mut()
            .ok_or(MidnightRuntimeError::NativeInternal)?
            .insert("expiresAt".to_owned(), serde_json::json!(expires_at));
    }
    to_json(&result)
}

fn submission_result(
    transaction_hash: &str,
    identifiers: Vec<String>,
    status: &str,
    body_base64: Option<String>,
) -> Result<String, MidnightRuntimeError> {
    let mut result = serde_json::json!({
        "transactionHash": transaction_hash,
        "identifiers": identifiers,
        "status": status,
    });
    if let Some(body_base64) = body_base64 {
        result
            .as_object_mut()
            .ok_or(MidnightRuntimeError::NativeInternal)?
            .insert("bodyBase64".to_owned(), serde_json::json!(body_base64));
    }
    to_json(&result)
}
