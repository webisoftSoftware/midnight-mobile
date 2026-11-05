use super::*;

mod command_coverage;
mod coverage;
mod dispatch;
mod extension;
mod hardening;
mod legacy_checkpoint;
mod operations;
mod sanitized;
mod transfer_finalization;

fn run_command(
    handle: &RuntimeSessionHandle,
    command: serde_json::Value,
) -> Result<serde_json::Value, MidnightRuntimeError> {
    begin_command(handle.id, handle.generation, command.to_string()).and_then(|value| {
        serde_json::from_str(&value).map_err(|_| MidnightRuntimeError::NativeInternal)
    })
}
