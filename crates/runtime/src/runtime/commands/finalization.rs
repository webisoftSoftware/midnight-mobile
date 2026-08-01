if let RuntimeCommand::FinalizeUnprovenTransaction {
    raw_base64,
    key_material,
} = &mut command
{
    let encoded = Zeroizing::new(std::mem::take(raw_base64));
    let raw = decode_base64(&encoded)?;
    let key_material = take_proving_key_material_map(key_material)?;
    let expected_identifiers =
        transaction::validate_unproven_transaction(&raw, &state.config.network_id)?;
    let network_id = state.config.network_id.clone();
    reserve_operation(&mut state)?;
    drop(state);
    return start_transaction_finalization(
        session_id,
        generation,
        &session,
        TransactionFinalizationInput {
            network_id,
            raw,
            key_material,
            proposed_state: None,
            expected_identifiers,
        },
    );
}
