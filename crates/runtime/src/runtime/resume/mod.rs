use super::*;

mod balance;
mod dapp;
mod finalize;
mod generate_dust;
mod transaction;

pub(super) fn resume_operation_kind(
    operation_id: u64,
    generation: u64,
    results: &[NetworkResult],
    result: &NetworkResult,
    operation: PendingOperation,
    state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    if matches!(
        &operation.kind,
        PendingOperationKind::DappIntentProof { .. }
    ) {
        return dapp::resume(operation_id, generation, results, operation, state);
    }
    if matches!(
        &operation.kind,
        PendingOperationKind::GenerateDustProof { .. }
    ) {
        return generate_dust::resume(operation_id, generation, results, operation, state);
    }
    if matches!(&operation.kind, PendingOperationKind::Balance { .. }) {
        return balance::resume(operation_id, generation, results, operation, state);
    }
    if matches!(
        &operation.kind,
        PendingOperationKind::FinalizeTransactionProof { .. }
    ) {
        return transaction::resume(operation_id, generation, results, operation, state);
    }
    finalize::resume(operation_id, generation, result, operation, state)
}
