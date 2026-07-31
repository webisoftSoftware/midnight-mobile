use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use super::{
    CircuitArtifact, LocalProverError, ParameterArtifact, close_registry, configure_registry,
    run_check, run_prove,
};

const MAX_PARAMETER_COUNT: usize = 32;
const MAX_CIRCUIT_COUNT: usize = 256;

#[repr(C)]
pub struct LocalProverParameterDescriptor {
    pub k: u32,
    pub bytes: *const u8,
    pub bytes_len: usize,
    pub sha256: *const u8,
}

#[repr(C)]
pub struct LocalProverCircuitDescriptor {
    pub key_location: *const u8,
    pub key_location_len: usize,
    pub prover_key: *const u8,
    pub prover_key_len: usize,
    pub prover_key_sha256: *const u8,
    pub verifier_key: *const u8,
    pub verifier_key_len: usize,
    pub verifier_key_sha256: *const u8,
    pub ir: *const u8,
    pub ir_len: usize,
    pub ir_sha256: *const u8,
}

#[repr(C)]
pub struct LocalProverResponse {
    pub bytes: *mut u8,
    pub bytes_len: usize,
}

impl Default for LocalProverResponse {
    fn default() -> Self {
        Self {
            bytes: ptr::null_mut(),
            bytes_len: 0,
        }
    }
}

impl LocalProverError {
    fn ffi_code(self) -> i32 {
        match self {
            Self::InvalidRequest => 1,
            Self::UnsupportedCircuit => 2,
            Self::IntegrityCheckFailed => 3,
            Self::ProverBusy => 4,
            Self::ResourcePreflightFailed => 5,
            Self::ProofFailed => 6,
            Self::InvalidConfiguration => 7,
            Self::StaleRegistry => 8,
            Self::CheckFailed => 9,
            Self::NativeInternal => 10,
        }
    }
}

unsafe fn required_slice<'a>(
    pointer: *const u8,
    length: usize,
) -> Result<&'a [u8], LocalProverError> {
    if pointer.is_null() || length == 0 {
        return Err(LocalProverError::ResourcePreflightFailed);
    }
    // SAFETY: The caller guarantees this pointer addresses length immutable bytes for the call.
    Ok(unsafe { std::slice::from_raw_parts(pointer, length) })
}

unsafe fn optional_array<'a, T>(
    pointer: *const T,
    length: usize,
    maximum: usize,
) -> Result<&'a [T], LocalProverError> {
    if length > maximum || (length > 0 && pointer.is_null()) {
        return Err(LocalProverError::InvalidConfiguration);
    }
    if length == 0 {
        return Ok(&[]);
    }
    // SAFETY: Count is bounded and the caller guarantees a readable contiguous descriptor array.
    Ok(unsafe { std::slice::from_raw_parts(pointer, length) })
}

unsafe fn parameter_inputs<'a>(
    pointer: *const LocalProverParameterDescriptor,
    count: usize,
) -> Result<Vec<ParameterArtifact<'a>>, LocalProverError> {
    // SAFETY: Descriptor validity is checked before any fields are read.
    let descriptors = unsafe { optional_array(pointer, count, MAX_PARAMETER_COUNT)? };
    descriptors
        .iter()
        .map(|descriptor| {
            let k =
                u8::try_from(descriptor.k).map_err(|_| LocalProverError::InvalidConfiguration)?;
            // SAFETY: Pointers remain caller-owned and readable for this synchronous call.
            let bytes = unsafe { required_slice(descriptor.bytes, descriptor.bytes_len)? };
            // SAFETY: Hash pointers address exactly 32 bytes by the C ABI contract.
            let sha256 = unsafe { required_slice(descriptor.sha256, 32)? };
            Ok(ParameterArtifact { k, bytes, sha256 })
        })
        .collect()
}

unsafe fn circuit_inputs<'a>(
    pointer: *const LocalProverCircuitDescriptor,
    count: usize,
) -> Result<Vec<CircuitArtifact<'a>>, LocalProverError> {
    // SAFETY: Descriptor validity is checked before any fields are read.
    let descriptors = unsafe { optional_array(pointer, count, MAX_CIRCUIT_COUNT)? };
    descriptors
        .iter()
        .map(|descriptor| {
            let location_pointer = descriptor.key_location;
            let location_length = descriptor.key_location_len;
            // SAFETY: Every pointer remains caller-owned and readable for this synchronous call.
            let location = unsafe { required_slice(location_pointer, location_length)? };
            let key_location = std::str::from_utf8(location)
                .map_err(|_| LocalProverError::InvalidConfiguration)?;
            // SAFETY: Artifact pointers and hashes satisfy the descriptor ABI contract.
            unsafe {
                Ok(CircuitArtifact {
                    key_location,
                    prover_key: required_slice(descriptor.prover_key, descriptor.prover_key_len)?,
                    prover_key_sha256: required_slice(descriptor.prover_key_sha256, 32)?,
                    verifier_key: required_slice(
                        descriptor.verifier_key,
                        descriptor.verifier_key_len,
                    )?,
                    verifier_key_sha256: required_slice(descriptor.verifier_key_sha256, 32)?,
                    ir: required_slice(descriptor.ir, descriptor.ir_len)?,
                    ir_sha256: required_slice(descriptor.ir_sha256, 32)?,
                })
            }
        })
        .collect()
}

fn guarded_code(operation: impl FnOnce() -> Result<(), LocalProverError>) -> i32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => error.ffi_code(),
        Err(_) => LocalProverError::NativeInternal.ffi_code(),
    }
}

fn leak_response(bytes: Vec<u8>) -> LocalProverResponse {
    let bytes = bytes.into_boxed_slice();
    let bytes_len = bytes.len();
    let bytes = Box::into_raw(bytes).cast::<u8>();
    LocalProverResponse { bytes, bytes_len }
}

fn guarded_response(
    output: &mut LocalProverResponse,
    operation: impl FnOnce() -> Result<Vec<u8>, LocalProverError>,
) -> i32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(bytes)) => {
            *output = leak_response(bytes);
            0
        }
        Ok(Err(error)) => error.ffi_code(),
        Err(_) => LocalProverError::NativeInternal.ffi_code(),
    }
}

/// Configures the process-wide local prover registry from caller-owned mapped artifacts.
///
/// # Safety
///
/// Descriptor arrays, every referenced byte range, and `output_handle` must remain valid for this
/// synchronous call. Hash pointers must address exactly 32 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_local_prover_configure(
    params: *const LocalProverParameterDescriptor,
    params_count: usize,
    circuits: *const LocalProverCircuitDescriptor,
    circuits_count: usize,
    output_handle: *mut u64,
) -> i32 {
    if output_handle.is_null() {
        return LocalProverError::InvalidConfiguration.ffi_code();
    }
    // SAFETY: The output pointer was checked non-null and caller guarantees writable storage.
    unsafe { *output_handle = 0 };
    guarded_code(|| {
        // SAFETY: Caller guarantees descriptor storage remains readable for this call.
        let params = unsafe { parameter_inputs(params, params_count)? };
        // SAFETY: Caller guarantees descriptor storage remains readable for this call.
        let circuits = unsafe { circuit_inputs(circuits, circuits_count)? };
        let handle = configure_registry(&params, &circuits)?;
        // SAFETY: The output pointer was validated and remains writable for this call.
        unsafe { *output_handle = handle };
        Ok(())
    })
}

unsafe fn run_request(
    handle: u64,
    request: *const u8,
    request_len: usize,
    output: *mut LocalProverResponse,
    operation: fn(u64, &[u8]) -> Result<Vec<u8>, LocalProverError>,
) -> i32 {
    if output.is_null() {
        return LocalProverError::InvalidRequest.ffi_code();
    }
    // SAFETY: Output pointer was checked non-null and caller guarantees writable storage.
    let output = unsafe { &mut *output };
    *output = LocalProverResponse::default();
    guarded_response(output, || {
        // SAFETY: Caller guarantees the request remains readable for this synchronous call.
        let request = unsafe { required_slice(request, request_len)? };
        operation(handle, request)
    })
}

/// Executes the official Ledger 8.1.0 `/check` request shape.
///
/// # Safety
///
/// Request and output pointers must remain valid for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_local_prover_check(
    handle: u64,
    request: *const u8,
    request_len: usize,
    output: *mut LocalProverResponse,
) -> i32 {
    // SAFETY: Safety requirements are identical to run_request and documented above.
    unsafe { run_request(handle, request, request_len, output, run_check) }
}

/// Executes the official Ledger 8.1.0 `/prove` request shape.
///
/// # Safety
///
/// Request and output pointers must remain valid for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_local_prover_prove(
    handle: u64,
    request: *const u8,
    request_len: usize,
    output: *mut LocalProverResponse,
) -> i32 {
    // SAFETY: Safety requirements are identical to run_request and documented above.
    unsafe { run_request(handle, request, request_len, output, run_prove) }
}

/// Clears the configured registry when `handle` is current.
#[unsafe(no_mangle)]
pub extern "C" fn midnight_local_prover_close(handle: u64) -> i32 {
    guarded_code(|| close_registry(handle))
}

/// Frees response bytes returned by check or prove.
///
/// # Safety
///
/// Pointer and length must be an unchanged pair returned by this library and freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_local_prover_free(bytes: *mut u8, bytes_len: usize) {
    if bytes.is_null() || bytes_len == 0 {
        return;
    }
    let slice = ptr::slice_from_raw_parts_mut(bytes, bytes_len);
    // SAFETY: Pointer and length came from leak_response and are consumed exactly once.
    drop(unsafe { Box::<[u8]>::from_raw(slice) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_error_codes_are_exhaustive() {
        let errors = [
            LocalProverError::InvalidRequest,
            LocalProverError::UnsupportedCircuit,
            LocalProverError::IntegrityCheckFailed,
            LocalProverError::ProverBusy,
            LocalProverError::ResourcePreflightFailed,
            LocalProverError::ProofFailed,
            LocalProverError::InvalidConfiguration,
            LocalProverError::StaleRegistry,
            LocalProverError::CheckFailed,
            LocalProverError::NativeInternal,
        ];
        assert_eq!(
            errors.map(LocalProverError::ffi_code),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        );
    }

    #[test]
    fn guards_contain_panics_and_manage_response_ownership() {
        assert_eq!(guarded_code(|| Ok(())), 0);
        assert_eq!(
            guarded_code(|| std::panic::resume_unwind(Box::new("panic"))),
            LocalProverError::NativeInternal.ffi_code()
        );
        let mut output = LocalProverResponse::default();
        assert_eq!(guarded_response(&mut output, || Ok(vec![1, 2, 3])), 0);
        assert_eq!(output.bytes_len, 3);
        // SAFETY: Pair is unchanged from guarded_response and freed once.
        unsafe { midnight_local_prover_free(output.bytes, output.bytes_len) };
    }

    #[test]
    fn null_and_oversized_descriptor_inputs_fail_closed() {
        let mut handle = 99_u64;
        // SAFETY: Null arrays intentionally exercise validation; handle is writable.
        let code = unsafe {
            midnight_local_prover_configure(
                ptr::null(),
                MAX_PARAMETER_COUNT + 1,
                ptr::null(),
                0,
                &mut handle,
            )
        };
        assert_eq!(code, LocalProverError::InvalidConfiguration.ffi_code());
        assert_eq!(handle, 0);
        let mut output = LocalProverResponse::default();
        // SAFETY: Null request intentionally exercises pointer validation.
        let code = unsafe { midnight_local_prover_check(1, ptr::null(), 0, &mut output) };
        assert_eq!(code, LocalProverError::ResourcePreflightFailed.ffi_code());
    }
}
