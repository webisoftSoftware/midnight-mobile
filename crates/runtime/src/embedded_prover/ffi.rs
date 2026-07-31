use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use super::{EmbeddedProofResult, EmbeddedProverError, run_embedded_prover_probe};

#[repr(C)]
pub struct EmbeddedProofFfiResult {
    pub tagged_proof_bytes: *mut u8,
    pub tagged_proof_bytes_len: usize,
    pub artifact_decoding_millis: u64,
    pub proving_and_self_verification_millis: u64,
}

#[repr(C)]
pub struct EmbeddedProofFfiInputs {
    pub request: *const u8,
    pub request_len: usize,
    pub params_k15: *const u8,
    pub params_k15_len: usize,
    pub prover_key: *const u8,
    pub prover_key_len: usize,
    pub verifier_key: *const u8,
    pub verifier_key_len: usize,
    pub ir: *const u8,
    pub ir_len: usize,
}

impl Default for EmbeddedProofFfiResult {
    fn default() -> Self {
        Self {
            tagged_proof_bytes: ptr::null_mut(),
            tagged_proof_bytes_len: 0,
            artifact_decoding_millis: 0,
            proving_and_self_verification_millis: 0,
        }
    }
}

impl EmbeddedProverError {
    fn ffi_code(self) -> i32 {
        match self {
            Self::InvalidRequest => 1,
            Self::UnsupportedCircuit => 2,
            Self::IntegrityCheckFailed => 3,
            Self::ProverBusy => 4,
            Self::ResourcePreflightFailed => 5,
            Self::ProofFailed => 6,
        }
    }
}

unsafe fn required_slice<'a>(pointer: *const u8, length: usize) -> Option<&'a [u8]> {
    if pointer.is_null() || length == 0 {
        return None;
    }
    // SAFETY: The caller promises that pointer addresses length readable bytes for this call.
    Some(unsafe { std::slice::from_raw_parts(pointer, length) })
}

fn leak_result(result: EmbeddedProofResult) -> EmbeddedProofFfiResult {
    let proof = result.tagged_proof_bytes.into_boxed_slice();
    let tagged_proof_bytes_len = proof.len();
    let tagged_proof_bytes = Box::into_raw(proof).cast::<u8>();
    EmbeddedProofFfiResult {
        tagged_proof_bytes,
        tagged_proof_bytes_len,
        artifact_decoding_millis: result.artifact_decoding_millis,
        proving_and_self_verification_millis: result.proving_and_self_verification_millis,
    }
}

fn run_probe(inputs: &EmbeddedProofFfiInputs) -> Result<EmbeddedProofFfiResult, i32> {
    // SAFETY: Input pointers are non-null and point to readable mappings for this synchronous call.
    let slices = unsafe {
        (
            required_slice(inputs.request, inputs.request_len),
            required_slice(inputs.params_k15, inputs.params_k15_len),
            required_slice(inputs.prover_key, inputs.prover_key_len),
            required_slice(inputs.verifier_key, inputs.verifier_key_len),
            required_slice(inputs.ir, inputs.ir_len),
        )
    };
    let (Some(request), Some(params), Some(prover), Some(verifier), Some(ir)) = slices else {
        return Err(EmbeddedProverError::ResourcePreflightFailed.ffi_code());
    };
    run_embedded_prover_probe(request, params, prover, verifier, ir)
        .map(leak_result)
        .map_err(EmbeddedProverError::ffi_code)
}

fn guarded_probe(
    output: &mut EmbeddedProofFfiResult,
    probe: impl FnOnce() -> Result<EmbeddedProofFfiResult, i32>,
) -> i32 {
    match catch_unwind(AssertUnwindSafe(probe)) {
        Ok(Ok(result)) => {
            *output = result;
            0
        }
        Ok(Err(code)) => code,
        Err(_) => EmbeddedProverError::ProofFailed.ffi_code(),
    }
}

/// Runs the embedded prover over caller-owned direct buffers.
///
/// # Safety
///
/// `inputs` and `output` must be valid aligned pointers. Every buffer described by `inputs` must
/// remain readable and unchanged until this synchronous call returns. `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_embedded_prover_probe(
    inputs: *const EmbeddedProofFfiInputs,
    output: *mut EmbeddedProofFfiResult,
) -> i32 {
    if inputs.is_null() || output.is_null() {
        return EmbeddedProverError::InvalidRequest.ffi_code();
    }
    // SAFETY: Both pointers were checked non-null; caller guarantees alignment and validity.
    let (inputs, output) = unsafe { (&*inputs, &mut *output) };
    *output = EmbeddedProofFfiResult::default();
    guarded_probe(output, || run_probe(inputs))
}

/// Releases proof bytes returned by [`midnight_embedded_prover_probe`].
///
/// # Safety
///
/// The pointer and length must be an unchanged pair returned by the probe and must be freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midnight_embedded_prover_free(
    tagged_proof_bytes: *mut u8,
    tagged_proof_bytes_len: usize,
) {
    if tagged_proof_bytes.is_null() || tagged_proof_bytes_len == 0 {
        return;
    }
    let slice = ptr::slice_from_raw_parts_mut(tagged_proof_bytes, tagged_proof_bytes_len);
    // SAFETY: The pointer and length came from leak_result and are freed exactly once here.
    drop(unsafe { Box::<[u8]>::from_raw(slice) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_rejects_null_control_pointers() {
        let mut output = EmbeddedProofFfiResult::default();
        // SAFETY: The null input is intentionally supplied to test validation.
        let code = unsafe { midnight_embedded_prover_probe(ptr::null(), &mut output) };
        assert_eq!(code, EmbeddedProverError::InvalidRequest.ffi_code());
    }

    #[test]
    fn ffi_rejects_missing_mappings_and_clears_output() {
        let inputs = EmbeddedProofFfiInputs {
            request: ptr::null(),
            request_len: 0,
            params_k15: ptr::null(),
            params_k15_len: 0,
            prover_key: ptr::null(),
            prover_key_len: 0,
            verifier_key: ptr::null(),
            verifier_key_len: 0,
            ir: ptr::null(),
            ir_len: 0,
        };
        let mut output = EmbeddedProofFfiResult {
            tagged_proof_bytes: ptr::dangling_mut(),
            tagged_proof_bytes_len: 1,
            artifact_decoding_millis: 1,
            proving_and_self_verification_millis: 1,
        };
        // SAFETY: Both control structures are valid for the duration of this call.
        let code = unsafe { midnight_embedded_prover_probe(&inputs, &mut output) };
        assert_eq!(
            code,
            EmbeddedProverError::ResourcePreflightFailed.ffi_code()
        );
        assert!(output.tagged_proof_bytes.is_null());
    }

    #[test]
    fn ffi_free_accepts_an_empty_result() {
        // SAFETY: A null, zero-length pair is explicitly accepted as a no-op.
        unsafe { midnight_embedded_prover_free(ptr::null_mut(), 0) };
    }

    #[test]
    fn ffi_codes_are_stable() {
        assert_eq!(EmbeddedProverError::InvalidRequest.ffi_code(), 1);
        assert_eq!(EmbeddedProverError::UnsupportedCircuit.ffi_code(), 2);
        assert_eq!(EmbeddedProverError::IntegrityCheckFailed.ffi_code(), 3);
        assert_eq!(EmbeddedProverError::ProverBusy.ffi_code(), 4);
        assert_eq!(EmbeddedProverError::ResourcePreflightFailed.ffi_code(), 5);
        assert_eq!(EmbeddedProverError::ProofFailed.ffi_code(), 6);
    }

    #[test]
    fn ffi_maps_nonempty_buffers_to_the_typed_probe_error() {
        let _serial = super::super::PROVER_TEST_LOCK.lock().unwrap();
        let byte = [1_u8];
        let inputs = EmbeddedProofFfiInputs {
            request: byte.as_ptr(),
            request_len: byte.len(),
            params_k15: byte.as_ptr(),
            params_k15_len: byte.len(),
            prover_key: byte.as_ptr(),
            prover_key_len: byte.len(),
            verifier_key: byte.as_ptr(),
            verifier_key_len: byte.len(),
            ir: byte.as_ptr(),
            ir_len: byte.len(),
        };
        let mut output = EmbeddedProofFfiResult::default();
        // SAFETY: The control structures and every one-byte input are valid for the call.
        let code = unsafe { midnight_embedded_prover_probe(&inputs, &mut output) };
        assert_eq!(code, EmbeddedProverError::InvalidRequest.ffi_code());
        assert!(output.tagged_proof_bytes.is_null());
    }

    #[test]
    fn ffi_result_ownership_round_trips() {
        let result = EmbeddedProofResult {
            tagged_proof_bytes: vec![1, 2, 3],
            artifact_decoding_millis: 4,
            proving_and_self_verification_millis: 5,
            proof_size: 3,
        };
        let leaked = leak_result(result);
        assert_eq!(leaked.tagged_proof_bytes_len, 3);
        assert_eq!(leaked.artifact_decoding_millis, 4);
        assert_eq!(leaked.proving_and_self_verification_millis, 5);
        // SAFETY: The pointer and length are the unchanged pair returned by leak_result.
        unsafe {
            assert_eq!(
                std::slice::from_raw_parts(
                    leaked.tagged_proof_bytes,
                    leaked.tagged_proof_bytes_len
                ),
                &[1, 2, 3]
            );
            midnight_embedded_prover_free(leaked.tagged_proof_bytes, leaked.tagged_proof_bytes_len);
        }
    }

    #[test]
    fn ffi_guard_contains_panics_and_writes_success() {
        let mut output = EmbeddedProofFfiResult::default();
        let code = guarded_probe(&mut output, || {
            Ok(EmbeddedProofFfiResult {
                tagged_proof_bytes: ptr::null_mut(),
                tagged_proof_bytes_len: 7,
                artifact_decoding_millis: 8,
                proving_and_self_verification_millis: 9,
            })
        });
        assert_eq!(code, 0);
        assert_eq!(output.tagged_proof_bytes_len, 7);

        let code = guarded_probe(&mut output, || {
            std::panic::resume_unwind(Box::new("synthetic native panic"))
        });
        assert_eq!(code, EmbeddedProverError::ProofFailed.ffi_code());
    }
}
