use super::*;

mod command_coverage;
mod coverage;
mod hardening;
mod operations;
mod sanitized;

fn run_command(
    handle: &RuntimeSessionHandle,
    command: serde_json::Value,
) -> Result<serde_json::Value, MidnightRuntimeError> {
    begin_command(handle.id, handle.generation, command.to_string()).and_then(|value| {
        serde_json::from_str(&value).map_err(|_| MidnightRuntimeError::NativeInternal)
    })
}
