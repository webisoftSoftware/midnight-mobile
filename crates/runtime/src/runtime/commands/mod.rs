use super::*;

mod codecs;
mod dapp;
mod deploy_and_balance;
mod finalization;
mod session_and_sync;
mod submission_and_queries;
mod transfers;

pub(super) fn begin_command_kind(
    session_id: u64,
    generation: u64,
    session: &Arc<Mutex<SessionState>>,
    command: RuntimeCommand,
    state: MutexGuard<'_, SessionState>,
) -> Result<String, MidnightRuntimeError> {
    match command {
        command @ (RuntimeCommand::CreateCheckPayload { .. }
        | RuntimeCommand::ParseCheckResult { .. }
        | RuntimeCommand::CreateProvingPayload { .. }
        | RuntimeCommand::CanonicalizeTransaction { .. }) => codecs::handle(command),
        command @ (RuntimeCommand::SignData { .. }
        | RuntimeCommand::CreateSyncRequest { .. }
        | RuntimeCommand::DeriveShieldedMintContext
        | RuntimeCommand::WatchShieldedMint { .. }
        | RuntimeCommand::CreateShieldedSpentRequest
        | RuntimeCommand::ApplyShieldedSpentResponse { .. }
        | RuntimeCommand::SetShieldedProtocolVersion { .. }
        | RuntimeCommand::CreateDustSpendRequest
        | RuntimeCommand::CreateDustCommitmentRequest { .. }
        | RuntimeCommand::ApplyDustSpendResolution { .. }) => {
            session_and_sync::handle(command, state)
        }
        command @ (RuntimeCommand::DappTransfer { .. } | RuntimeCommand::DappIntent { .. }) => {
            dapp::handle(session_id, generation, session, command, state)
        }
        command @ (RuntimeCommand::Transfer { .. } | RuntimeCommand::GenerateDust { .. }) => {
            transfers::handle(session_id, generation, session, command, state)
        }
        command @ RuntimeCommand::FinalizeUnprovenTransaction { .. } => {
            finalization::handle(session_id, generation, session, command, state)
        }
        command @ (RuntimeCommand::BalanceUnsealed { .. }
        | RuntimeCommand::BalanceSealed { .. }) => {
            deploy_and_balance::handle(session_id, generation, session, command, state)
        }
        command @ RuntimeCommand::SubmitFinalized { .. } => {
            submission_and_queries::handle(session_id, generation, session, command, state)
        }
    }
}
