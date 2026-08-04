use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use super::{
    CircuitArtifact, LocalProverError, ParameterArtifact, cancel_all, close_registry,
    configure_registry, run_check, run_prove, run_prove_batch, set_max_concurrency, set_profiling,
    take_timings,
};
use crate::transaction::MAX_PROOF_BATCH;

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
pub struct LocalProverRequestDescriptor {
    pub bytes: *const u8,
    pub bytes_len: usize,
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

unsafe fn batch_request_inputs<'a>(
    pointer: *const LocalProverRequestDescriptor,
    count: usize,
) -> Result<Vec<&'a [u8]>, LocalProverError> {
    // SAFETY: Validity, including the MAX_PROOF_BATCH cap, is checked before any field is read.
    let descriptors = unsafe { optional_array(pointer, count, MAX_PROOF_BATCH)? };
    descriptors
        .iter()
        .map(|descriptor| {
            // SAFETY: Every pointer remains caller-owned and readable for this synchronous call.
            unsafe { required_slice(descriptor.bytes, descriptor.bytes_len) }
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
pub unsafe extern "C" fn midnight_mobile_local_prover_configure(
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
pub unsafe extern "C" fn midnight_mobile_local_prover_check(
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
pub unsafe extern "C" fn midnight_mobile_local_prover_prove(
    handle: u64,
    request: *const u8,
    request_len: usize,
    output: *mut LocalProverResponse,
) -> i32 {
    // SAFETY: Safety requirements are identical to run_request and documented above.
    unsafe { run_request(handle, request, request_len, output, run_prove) }
}

/// Executes the official Ledger 8.1.0 `/prove` request shape for up to `MAX_PROOF_BATCH` requests
/// in one call. Fans out inside Rust so a single admission decision, the shared bounded-permit
/// gate, and the shared Rayon pool govern every request the same way they govern a single
/// `midnight_mobile_local_prover_prove` call; see `run_prove_batch`'s doc comment.
///
/// Ownership is all-or-nothing: `outputs` is zeroed at entry, and on any failure every
/// `outputs[i]` is left as that null default -- no response is ever leaked without a corresponding
/// live slot the caller can free. On success every slot holds a leaked buffer to be freed with
/// `midnight_mobile_local_prover_free`, exactly like the single-request entrypoints.
///
/// # Safety
///
/// `requests`, every descriptor's referenced byte range, and `outputs` must remain valid for this
/// synchronous call. When `request_count` is nonzero, `outputs` must address at least
/// `request_count` writable `LocalProverResponse` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_mobile_local_prover_prove_batch(
    handle: u64,
    requests: *const LocalProverRequestDescriptor,
    request_count: usize,
    outputs: *mut LocalProverResponse,
) -> i32 {
    if request_count > MAX_PROOF_BATCH {
        return LocalProverError::InvalidConfiguration.ffi_code();
    }
    if request_count == 0 {
        return 0;
    }
    if outputs.is_null() {
        return LocalProverError::InvalidRequest.ffi_code();
    }
    // SAFETY: Output was checked non-null and the caller guarantees request_count writable slots.
    let outputs = unsafe { std::slice::from_raw_parts_mut(outputs, request_count) };
    for output in outputs.iter_mut() {
        *output = LocalProverResponse::default();
    }
    guarded_code(|| {
        // SAFETY: Caller guarantees descriptor storage remains readable for this call.
        let requests = unsafe { batch_request_inputs(requests, request_count)? };
        let responses = run_prove_batch(handle, &requests)?;
        for (output, bytes) in outputs.iter_mut().zip(responses) {
            *output = leak_response(bytes);
        }
        Ok(())
    })
}

/// Bumps the process-wide cancellation epoch so batch workers (see
/// `midnight_mobile_local_prover_prove_batch`) that have not yet started their individual proof
/// abandon it. Cannot interrupt a proof already inside `preimage.prove(...)`, which is pinned
/// upstream ledger code with no cancellation token; `handle` is accepted for interface symmetry
/// with the other entrypoints, but cancellation itself is process-wide since only one registry is
/// ever configured at a time.
#[unsafe(no_mangle)]
pub extern "C" fn midnight_mobile_local_prover_cancel(handle: u64) -> i32 {
    let _ = handle;
    cancel_all();
    0
}

/// Clears the configured registry when `handle` is current.
#[unsafe(no_mangle)]
pub extern "C" fn midnight_mobile_local_prover_close(handle: u64) -> i32 {
    guarded_code(|| close_registry(handle))
}

/// Enables or disables per-proof stage timing collection (Phase 1 instrumentation). Disabled by
/// default; when enabled, `run_prove` additionally times the exact per-proof work a typed
/// initialized-circuit cache would eliminate, so only enable this for measurement, not in normal
/// operation.
#[unsafe(no_mangle)]
pub extern "C" fn midnight_mobile_local_prover_set_profiling(enabled: bool) -> i32 {
    set_profiling(enabled);
    0
}

/// Drains recorded stage-timing samples as a UTF-8 JSON array through the usual
/// `LocalProverResponse` ownership contract; free with `midnight_mobile_local_prover_free`. Emits
/// `[]` when there are no samples.
///
/// # Safety
///
/// `output` must remain valid for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_mobile_local_prover_take_timings(
    output: *mut LocalProverResponse,
) -> i32 {
    if output.is_null() {
        return LocalProverError::InvalidRequest.ffi_code();
    }
    // SAFETY: Output pointer was checked non-null and caller guarantees writable storage.
    let output = unsafe { &mut *output };
    *output = LocalProverResponse::default();
    guarded_response(output, take_timings)
}

/// Sets the maximum number of concurrent shared permits (`run_check`/`run_prove`), clamped to
/// `1..=4`. Does not affect in-flight permits already granted.
#[unsafe(no_mangle)]
pub extern "C" fn midnight_mobile_local_prover_set_max_concurrency(limit: usize) -> i32 {
    set_max_concurrency(limit);
    0
}

/// Frees response bytes returned by check or prove.
///
/// # Safety
///
/// Pointer and length must be an unchanged pair returned by this library and freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_mobile_local_prover_free(bytes: *mut u8, bytes_len: usize) {
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
            guarded_code(|| Err(LocalProverError::InvalidConfiguration)),
            LocalProverError::InvalidConfiguration.ffi_code()
        );
        assert_eq!(
            guarded_code(|| std::panic::resume_unwind(Box::new("panic"))),
            LocalProverError::NativeInternal.ffi_code()
        );
        let mut output = LocalProverResponse::default();
        assert_eq!(guarded_response(&mut output, || Ok(vec![1, 2, 3])), 0);
        assert_eq!(output.bytes_len, 3);
        // SAFETY: Pair is unchanged from guarded_response and freed once.
        unsafe { midnight_mobile_local_prover_free(output.bytes, output.bytes_len) };

        let mut output = LocalProverResponse::default();
        assert_eq!(
            guarded_response(&mut output, || Err(LocalProverError::CheckFailed)),
            LocalProverError::CheckFailed.ffi_code()
        );
        assert!(output.bytes.is_null());
        assert_eq!(
            guarded_response(&mut output, || {
                std::panic::resume_unwind(Box::new("panic"))
            }),
            LocalProverError::NativeInternal.ffi_code()
        );
        // SAFETY: Null pointers are explicitly accepted as no-ops.
        unsafe { midnight_mobile_local_prover_free(ptr::null_mut(), 7) };
    }

    #[test]
    fn pointer_and_descriptor_decoders_cover_valid_and_invalid_shapes() {
        let bytes = [1_u8, 2];
        let hash = [3_u8; 32];
        assert_eq!(
            // SAFETY: Every pointer addresses the corresponding live local array.
            unsafe { required_slice(bytes.as_ptr(), bytes.len()) }.unwrap(),
            bytes
        );
        assert!(
            // SAFETY: A zero-length optional array does not dereference its pointer.
            unsafe { optional_array::<u8>(ptr::null(), 0, 1) }
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            // SAFETY: Pointer addresses one live value and the bound permits one item.
            unsafe { optional_array(bytes.as_ptr(), 1, 1) }.unwrap(),
            &[1]
        );
        assert!(matches!(
            // SAFETY: Null pointer is intentionally rejected before dereference.
            unsafe { optional_array::<u8>(ptr::null(), 1, 1) },
            Err(LocalProverError::InvalidConfiguration)
        ));

        let parameter = LocalProverParameterDescriptor {
            k: 7,
            bytes: bytes.as_ptr(),
            bytes_len: bytes.len(),
            sha256: hash.as_ptr(),
        };
        // SAFETY: Descriptor and all referenced arrays remain live for decoding.
        let decoded = unsafe { parameter_inputs(&parameter, 1) }.unwrap();
        assert_eq!(decoded[0].k, 7);
        assert_eq!(decoded[0].bytes, bytes);
        assert_eq!(decoded[0].sha256, hash);

        let invalid_k = LocalProverParameterDescriptor {
            k: 256,
            bytes: bytes.as_ptr(),
            bytes_len: bytes.len(),
            sha256: hash.as_ptr(),
        };
        assert!(matches!(
            // SAFETY: Descriptor is live and fails before referenced data is consumed.
            unsafe { parameter_inputs(&invalid_k, 1) },
            Err(LocalProverError::InvalidConfiguration)
        ));
        let missing_bytes = LocalProverParameterDescriptor {
            k: 7,
            bytes: ptr::null(),
            bytes_len: bytes.len(),
            sha256: hash.as_ptr(),
        };
        assert!(matches!(
            // SAFETY: Null artifact pointer is intentionally rejected before dereference.
            unsafe { parameter_inputs(&missing_bytes, 1) },
            Err(LocalProverError::ResourcePreflightFailed)
        ));

        let location = b"midnight/test";
        let circuit = LocalProverCircuitDescriptor {
            key_location: location.as_ptr(),
            key_location_len: location.len(),
            prover_key: bytes.as_ptr(),
            prover_key_len: bytes.len(),
            prover_key_sha256: hash.as_ptr(),
            verifier_key: bytes.as_ptr(),
            verifier_key_len: bytes.len(),
            verifier_key_sha256: hash.as_ptr(),
            ir: bytes.as_ptr(),
            ir_len: bytes.len(),
            ir_sha256: hash.as_ptr(),
        };
        // SAFETY: Descriptor and all referenced arrays remain live for decoding.
        let decoded = unsafe { circuit_inputs(&circuit, 1) }.unwrap();
        assert_eq!(decoded[0].key_location, "midnight/test");
        assert_eq!(decoded[0].prover_key, bytes);

        let invalid_utf8 = [0xff_u8];
        let malformed = LocalProverCircuitDescriptor {
            key_location: invalid_utf8.as_ptr(),
            key_location_len: invalid_utf8.len(),
            ..circuit
        };
        assert!(matches!(
            // SAFETY: Location pointer is live but contains invalid UTF-8.
            unsafe { circuit_inputs(&malformed, 1) },
            Err(LocalProverError::InvalidConfiguration)
        ));
        assert!(matches!(
            // SAFETY: Oversized count is rejected before dereferencing the null pointer.
            unsafe { circuit_inputs(ptr::null(), MAX_CIRCUIT_COUNT + 1) },
            Err(LocalProverError::InvalidConfiguration)
        ));
    }

    #[test]
    fn null_and_oversized_descriptor_inputs_fail_closed() {
        let _serial = super::super::PROVER_TEST_LOCK.lock().unwrap();
        super::super::registry_slot().lock().unwrap().value = None;
        assert_eq!(
            // SAFETY: Null output handle is intentionally rejected before dereference.
            unsafe {
                midnight_mobile_local_prover_configure(
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                    ptr::null_mut(),
                )
            },
            LocalProverError::InvalidConfiguration.ffi_code()
        );
        let mut handle = 99_u64;
        // SAFETY: Null arrays intentionally exercise validation; handle is writable.
        let code = unsafe {
            midnight_mobile_local_prover_configure(
                ptr::null(),
                MAX_PARAMETER_COUNT + 1,
                ptr::null(),
                0,
                &mut handle,
            )
        };
        assert_eq!(code, LocalProverError::InvalidConfiguration.ffi_code());
        assert_eq!(handle, 0);
        // SAFETY: Empty arrays are valid pointer shapes and fail core configuration validation.
        let code = unsafe {
            midnight_mobile_local_prover_configure(ptr::null(), 0, ptr::null(), 0, &mut handle)
        };
        assert_eq!(code, LocalProverError::InvalidConfiguration.ffi_code());

        let bytes = [0_u8; 4];
        let wrong_hash = [0_u8; 32];
        let parameter = LocalProverParameterDescriptor {
            k: 0,
            bytes: bytes.as_ptr(),
            bytes_len: bytes.len(),
            sha256: wrong_hash.as_ptr(),
        };
        // SAFETY: Descriptor and referenced buffers remain live for the synchronous call.
        let code = unsafe {
            midnight_mobile_local_prover_configure(&parameter, 1, ptr::null(), 0, &mut handle)
        };
        assert_eq!(code, LocalProverError::IntegrityCheckFailed.ffi_code());

        let mut output = LocalProverResponse::default();
        // SAFETY: Null request intentionally exercises pointer validation.
        let code = unsafe { midnight_mobile_local_prover_check(1, ptr::null(), 0, &mut output) };
        assert_eq!(code, LocalProverError::ResourcePreflightFailed.ffi_code());
        assert_eq!(
            // SAFETY: Null output is rejected before the live request is dereferenced.
            unsafe { midnight_mobile_local_prover_check(1, bytes.as_ptr(), 1, ptr::null_mut()) },
            LocalProverError::InvalidRequest.ffi_code()
        );
        assert_eq!(
            // SAFETY: Request and output remain live; missing registry fails closed.
            unsafe { midnight_mobile_local_prover_check(1, bytes.as_ptr(), 1, &mut output) },
            LocalProverError::StaleRegistry.ffi_code()
        );
        assert_eq!(
            // SAFETY: Request and output remain live; missing registry fails closed.
            unsafe { midnight_mobile_local_prover_prove(1, bytes.as_ptr(), 1, &mut output) },
            LocalProverError::StaleRegistry.ffi_code()
        );
        assert_eq!(
            midnight_mobile_local_prover_close(0),
            LocalProverError::StaleRegistry.ffi_code()
        );
    }

    #[test]
    fn batch_descriptor_decoding_fails_closed_on_null_oversized_and_empty_shapes() {
        assert!(matches!(
            // SAFETY: A zero-length array is a valid shape and never dereferences the pointer.
            unsafe { batch_request_inputs(ptr::null(), 0) },
            Ok(decoded) if decoded.is_empty()
        ));
        assert!(matches!(
            // SAFETY: A null pointer with a nonzero count is rejected before any dereference.
            unsafe { batch_request_inputs(ptr::null(), 1) },
            Err(LocalProverError::InvalidConfiguration)
        ));
        assert!(matches!(
            // SAFETY: An oversized count is rejected before the null pointer is dereferenced.
            unsafe { batch_request_inputs(ptr::null(), MAX_PROOF_BATCH + 1) },
            Err(LocalProverError::InvalidConfiguration)
        ));

        let bytes = [1_u8, 2, 3];
        let null_request = LocalProverRequestDescriptor {
            bytes: ptr::null(),
            bytes_len: bytes.len(),
        };
        assert!(matches!(
            // SAFETY: Descriptor is live; its null request pointer is rejected before dereference.
            unsafe { batch_request_inputs(&null_request, 1) },
            Err(LocalProverError::ResourcePreflightFailed)
        ));

        let empty_request = LocalProverRequestDescriptor {
            bytes: bytes.as_ptr(),
            bytes_len: 0,
        };
        assert!(matches!(
            // SAFETY: Descriptor and its buffer are live; the zero length is rejected.
            unsafe { batch_request_inputs(&empty_request, 1) },
            Err(LocalProverError::ResourcePreflightFailed)
        ));

        let valid_request = LocalProverRequestDescriptor {
            bytes: bytes.as_ptr(),
            bytes_len: bytes.len(),
        };
        // SAFETY: Descriptor and buffer remain live for decoding.
        let decoded = unsafe { batch_request_inputs(&valid_request, 1) }.unwrap();
        assert_eq!(decoded[0], bytes);
    }

    #[test]
    fn prove_batch_nulls_outputs_at_entry_and_leaves_them_null_on_a_decode_failure() {
        let _serial = super::super::PROVER_TEST_LOCK.lock().unwrap();
        let sentinel_backing = [9_u8; 4];
        let mut outputs = [
            LocalProverResponse {
                bytes: sentinel_backing.as_ptr().cast_mut(),
                bytes_len: sentinel_backing.len(),
            },
            LocalProverResponse {
                bytes: sentinel_backing.as_ptr().cast_mut(),
                bytes_len: sentinel_backing.len(),
            },
        ];
        // The request array is null with a nonzero count, so decoding fails inside guarded_code --
        // after the entry zeroing and before any request is read.
        // SAFETY: The output array is live and writable; the null request array is rejected first.
        let code = unsafe {
            midnight_mobile_local_prover_prove_batch(1, ptr::null(), 2, outputs.as_mut_ptr())
        };
        assert_eq!(code, LocalProverError::InvalidConfiguration.ffi_code());
        for output in &outputs {
            assert!(output.bytes.is_null());
            assert_eq!(output.bytes_len, 0);
        }
    }

    #[test]
    fn prove_batch_fails_closed_on_a_stale_handle_and_frees_nothing() {
        let _serial = super::super::PROVER_TEST_LOCK.lock().unwrap();
        super::super::registry_slot().lock().unwrap().value = None;
        let bytes = [7_u8; 4];
        let descriptors = [
            LocalProverRequestDescriptor {
                bytes: bytes.as_ptr(),
                bytes_len: bytes.len(),
            },
            LocalProverRequestDescriptor {
                bytes: bytes.as_ptr(),
                bytes_len: bytes.len(),
            },
        ];
        let mut outputs = [
            LocalProverResponse::default(),
            LocalProverResponse::default(),
        ];
        // SAFETY: Descriptors, their request buffers, and the output array all remain live.
        let code = unsafe {
            midnight_mobile_local_prover_prove_batch(
                1,
                descriptors.as_ptr(),
                descriptors.len(),
                outputs.as_mut_ptr(),
            )
        };
        assert_eq!(code, LocalProverError::StaleRegistry.ffi_code());
        for output in &outputs {
            assert!(output.bytes.is_null());
        }
    }

    #[test]
    fn prove_batch_rejects_an_oversized_request_count_before_touching_memory() {
        let mut outputs = [LocalProverResponse::default()];
        // SAFETY: The oversized count is rejected before the null request array is dereferenced.
        let code = unsafe {
            midnight_mobile_local_prover_prove_batch(
                1,
                ptr::null(),
                MAX_PROOF_BATCH + 1,
                outputs.as_mut_ptr(),
            )
        };
        assert_eq!(code, LocalProverError::InvalidConfiguration.ffi_code());
    }

    #[test]
    fn prove_batch_of_zero_requests_is_a_trivial_success() {
        let outputs = ptr::null_mut();
        // SAFETY: A zero request count reads neither the request array nor the output array.
        let code = unsafe { midnight_mobile_local_prover_prove_batch(1, ptr::null(), 0, outputs) };
        assert_eq!(code, 0);
    }

    #[test]
    fn cancel_bumps_the_process_wide_epoch() {
        let _serial = super::super::PROVER_TEST_LOCK.lock().unwrap();
        let before = super::super::CANCEL_EPOCH.load(std::sync::atomic::Ordering::Acquire);
        assert_eq!(midnight_mobile_local_prover_cancel(1), 0);
        let after = super::super::CANCEL_EPOCH.load(std::sync::atomic::Ordering::Acquire);
        assert_eq!(after, before + 1);
    }
}
