use super::super::*;

pub(super) fn handle(mut command: RuntimeCommand) -> Result<String, MidnightRuntimeError> {
    match &mut command {
        RuntimeCommand::CreateCheckPayload {
            preimage_base64,
            ir_base64,
        } => {
            let preimage = decode_base64(preimage_base64);
            preimage_base64.zeroize();
            let preimage = Zeroizing::new(preimage?);
            let ir = ir_base64
                .as_mut()
                .map(|value| {
                    let decoded = decode_base64(value);
                    value.zeroize();
                    decoded.map(Zeroizing::new)
                })
                .transpose()?;
            let payload =
                codec::create_check_payload(&preimage, ir.as_deref().map(|value| value.to_vec()))?;
            return complete_json(to_json(&serde_json::json!({
                "payloadBase64": encode_base64(&payload),
            }))?);
        }
        RuntimeCommand::ParseCheckResult { result_base64 } => {
            let raw = decode_base64(result_base64)?;
            let values = codec::parse_check_result(&raw)?
                .into_iter()
                .map(|value| value.map(|value| value.to_string()))
                .collect::<Vec<_>>();
            return complete_json(to_json(&serde_json::json!({ "values": values }))?);
        }
        RuntimeCommand::CreateProvingPayload {
            preimage_base64,
            binding_input,
            key_material,
        } => {
            let preimage = decode_base64(preimage_base64);
            preimage_base64.zeroize();
            let preimage = Zeroizing::new(preimage?);
            let key_material = take_proving_key_material(key_material)?;
            let payload =
                codec::create_proving_payload(&preimage, binding_input.as_deref(), key_material)?;
            return complete_json(to_json(&serde_json::json!({
                "payloadBase64": encode_base64(&payload),
            }))?);
        }
        RuntimeCommand::CanonicalizeTransaction {
            signature_marker,
            proof_marker,
            binding_marker,
            raw_base64,
        } => {
            let raw = decode_base64(raw_base64)?;
            let canonical = transaction::canonicalize_transaction(
                signature_marker,
                proof_marker,
                binding_marker,
                &raw,
            )?;
            return complete_json(to_json(&serde_json::json!({
                "transactionBase64": encode_base64(&canonical),
            }))?);
        }
        _ => {}
    }
    Err(MidnightRuntimeError::InvalidArgument)
}
