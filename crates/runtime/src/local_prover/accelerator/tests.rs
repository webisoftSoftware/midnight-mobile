//! The accelerator switch, the CPU retry and real wallet proofs on the device.
//!
//! Tests that need a GPU pass without checking anything on a machine that has
//! none, unless `MIDNIGHT_MOBILE_REQUIRE_GPU=1` is set.

use std::sync::MutexGuard;

use midnight_curves::Fq;
use midnight_ledger::structure::ProofVersioned;
use midnight_proofs::poly::EvaluationDomain;
use midnight_serialize::tagged_deserialize;
use sha2::{Digest, Sha256};

use super::super::{
    CircuitArtifact, LocalProverError, PROVER_TEST_LOCK, ParameterArtifact, close_registry,
    configure_registry, deterministic_zswap_output_request, deterministic_zswap_spend_request,
    run_prove, run_prove_batch, set_profiling, take_timings,
};
use super::{accelerator_status, midnight_mobile_local_prover_trim_memory, prove, set_accelerator};

/// Serializes the test with the other prover tests and leaves the accelerator
/// off, as a new process has it.
struct Serial(#[allow(dead_code)] MutexGuard<'static, ()>);

impl Serial {
    fn take() -> Self {
        let guard = PROVER_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_accelerator(false);
        Self(guard)
    }
}

impl Drop for Serial {
    fn drop(&mut self) {
        set_accelerator(false);
        set_profiling(false);
    }
}

fn status() -> serde_json::Value {
    serde_json::from_slice(&accelerator_status().unwrap()).unwrap()
}

/// Turns the accelerator on. `false`: this machine has no usable GPU and the
/// caller returns without checking anything.
fn gpu_is_active() -> bool {
    set_accelerator(true);
    let active = status()["active"] == true;
    assert!(
        active || std::env::var("MIDNIGHT_MOBILE_REQUIRE_GPU").as_deref() != Ok("1"),
        "a GPU is required: {}",
        status()
    );
    active
}

/// A transform large enough for the device at its default thresholds.
fn device_work() {
    let domain = EvaluationDomain::<Fq>::new(1, 15);
    let values = (0..1u64 << 15).map(Fq::from).collect();
    let _ = domain.coeff_to_lagrange(domain.coeff_from_vec(values));
}

#[test]
fn the_accelerator_is_off_until_the_host_asks() {
    let _serial = Serial::take();
    let status = status();
    assert_eq!(status["requested"], false);
    assert_eq!(status["availability"], "not-requested");
    assert_eq!(status["active"], false);
    assert_eq!(status["tripped"], false);
    assert!(status["device"].is_null());
    for field in [
        "gpuOperations",
        "cpuOperations",
        "deviceMicros",
        "cpuRetries",
        "tables",
    ] {
        assert!(status[field].is_u64(), "{field} in {status}");
    }
}

#[test]
fn asking_for_the_accelerator_reports_why_it_is_or_is_not_used() {
    let _serial = Serial::take();
    set_accelerator(true);
    let status = status();
    assert_eq!(status["requested"], true);
    let availability = status["availability"].as_str().unwrap();
    assert!(["active", "unsupported", "no-device", "hooks-taken"].contains(&availability));
    assert_eq!(status["active"], availability == "active");
    assert_eq!(status["device"].is_string(), availability == "active");

    set_accelerator(false);
    let off = self::status();
    assert_eq!(off["requested"], false);
    assert_eq!(off["active"], false);
}

#[test]
fn trimming_memory_is_safe_in_every_state_and_frees_the_tables() {
    let _serial = Serial::take();
    // Off, never requested in this state: nothing to free, still a success.
    assert_eq!(midnight_mobile_local_prover_trim_memory(), 0);
    if !gpu_is_active() {
        return;
    }
    let (_, usage) = prove(|| {
        device_work();
        Ok::<_, LocalProverError>(())
    });
    assert!(usage.gpu_operations > 0);
    assert_eq!(midnight_mobile_local_prover_trim_memory(), 0);
    let status = status();
    assert_eq!(status["tables"], 0);
    assert_eq!(
        status["active"], true,
        "trimming does not switch the accelerator off"
    );
    // The next proof allocates again and still uses the device.
    let (_, usage) = prove(|| {
        device_work();
        Ok::<_, LocalProverError>(())
    });
    assert!(usage.gpu_operations > 0);
}

#[test]
fn an_attempt_runs_once_without_the_device() {
    let _serial = Serial::take();
    let mut attempts = 0;
    let (result, usage) = prove(|| {
        attempts += 1;
        Err::<(), _>(LocalProverError::ProofFailed)
    });
    assert_eq!(result, Err(LocalProverError::ProofFailed));
    assert_eq!(attempts, 1, "no retry when the device did nothing");
    assert_eq!(usage.gpu_operations, 0);

    let (result, _) = prove(|| Ok::<_, LocalProverError>(7));
    assert_eq!(result, Ok(7));
}

#[test]
fn a_failure_after_device_work_is_retried_on_the_cpu_and_trips_the_accelerator() {
    let _serial = Serial::take();
    if !gpu_is_active() {
        return;
    }
    let retries_before = status()["cpuRetries"].as_u64().unwrap();
    let mut attempts = 0;
    let (result, usage) = prove(|| {
        attempts += 1;
        device_work();
        // The first attempt stands for a proof the device spoiled.
        if attempts == 1 {
            Err(LocalProverError::ProofFailed)
        } else {
            Ok(attempts)
        }
    });
    assert_eq!(result, Ok(2));
    assert!(
        usage.gpu_operations > 0,
        "the first attempt used the device"
    );
    let tripped = status();
    assert_eq!(tripped["tripped"], true);
    assert_eq!(tripped["active"], false);
    assert_eq!(tripped["cpuRetries"].as_u64().unwrap(), retries_before + 1);

    // While tripped, proofs do not touch the device.
    let (_, usage) = prove(|| {
        device_work();
        Ok::<_, LocalProverError>(())
    });
    assert_eq!(usage.gpu_operations, 0);

    // The host re-arms it by asking again.
    set_accelerator(true);
    let rearmed = status();
    assert_eq!(rearmed["tripped"], false);
    assert_eq!(rearmed["active"], true);
}

#[test]
fn a_request_that_fails_on_the_cpu_too_leaves_the_accelerator_on() {
    let _serial = Serial::take();
    if !gpu_is_active() {
        return;
    }
    let mut attempts = 0;
    let (result, _) = prove(|| {
        attempts += 1;
        device_work();
        Err::<(), _>(LocalProverError::ProofFailed)
    });
    assert_eq!(result, Err(LocalProverError::ProofFailed));
    assert_eq!(attempts, 2, "one retry on the CPU");
    let status = status();
    assert_eq!(status["tripped"], false);
    assert_eq!(status["active"], true);
}

struct Staged {
    files: Vec<Vec<u8>>,
    hashes: Vec<Vec<u8>>,
}

impl Staged {
    const NAMES: [&'static str; 8] = [
        "bls_midnight_2p15",
        "bls_midnight_2p14",
        "zswap/9/spend.prover",
        "zswap/9/spend.verifier",
        "zswap/9/spend.bzkir",
        "zswap/9/output.prover",
        "zswap/9/output.verifier",
        "zswap/9/output.bzkir",
    ];

    fn read() -> Self {
        let directory = std::path::PathBuf::from(
            std::env::var_os("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR")
                .expect("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR must be set"),
        );
        let files: Vec<Vec<u8>> = Self::NAMES
            .iter()
            .map(|name| std::fs::read(directory.join(name)).unwrap())
            .collect();
        let hashes = files
            .iter()
            .map(|bytes| Sha256::digest(bytes).to_vec())
            .collect();
        Self { files, hashes }
    }

    fn circuit(&self, key_location: &'static str, first: usize) -> CircuitArtifact<'_> {
        CircuitArtifact {
            key_location,
            prover_key: &self.files[first],
            prover_key_sha256: &self.hashes[first],
            verifier_key: &self.files[first + 1],
            verifier_key_sha256: &self.hashes[first + 1],
            ir: &self.files[first + 2],
            ir_sha256: &self.hashes[first + 2],
        }
    }

    /// Configures the Zswap spend (k = 15) and output (k = 14) circuits.
    fn configure(&self) -> u64 {
        configure_registry(
            &[
                ParameterArtifact {
                    k: 15,
                    bytes: &self.files[0],
                    sha256: &self.hashes[0],
                },
                ParameterArtifact {
                    k: 14,
                    bytes: &self.files[1],
                    sha256: &self.hashes[1],
                },
            ],
            &[
                self.circuit("midnight/zswap/spend", 2),
                self.circuit("midnight/zswap/output", 5),
            ],
        )
        .unwrap()
    }
}

/// The accelerator's part in each proof recorded since the last call.
fn recorded_gpu_operations() -> Vec<u64> {
    let samples: serde_json::Value = serde_json::from_slice(&take_timings().unwrap()).unwrap();
    samples
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["accelerator"]["gpuOperations"].as_u64().unwrap())
        .collect()
}

fn is_a_proof(response: &[u8]) -> bool {
    matches!(
        tagged_deserialize::<ProofVersioned>(&mut &response[..]),
        Ok(ProofVersioned::V2(_))
    )
}

/// Real wallet circuits with the official keys. `run_prove` only returns a
/// proof that passed the ledger's verification against the official verifier
/// key, so every `Ok` below is a proof the network accepts.
#[test]
#[ignore = "requires staged public artifacts"]
fn accelerated_zswap_proofs_are_accepted_by_the_official_verifier_keys() {
    let _serial = Serial::take();
    let staged = Staged::read();
    let handle = staged.configure();
    let spend = deterministic_zswap_spend_request().unwrap();
    let output = deterministic_zswap_output_request().unwrap();
    set_profiling(true);
    let _ = take_timings();

    // The CPU prover, for reference.
    assert!(is_a_proof(&run_prove(handle, &spend).unwrap()));
    assert_eq!(recorded_gpu_operations(), [0]);

    if gpu_is_active() {
        // Switching the accelerator on prepared the tables of the configured
        // circuits: the first proof does not pay for them.
        assert_eq!(status()["tables"], 3, "{}", status());
        // The counter is the process's: other tests trip the accelerator.
        let retries_before = status()["cpuRetries"].clone();
        assert!(is_a_proof(&run_prove(handle, &spend).unwrap()));
        assert!(is_a_proof(&run_prove(handle, &output).unwrap()));
        let used = recorded_gpu_operations();
        assert_eq!(used.len(), 2);
        assert!(
            used.iter().all(|&operations| operations > 0),
            "both proofs used the GPU: {used:?}"
        );

        // A send as the wallet makes it: several proofs in one batch.
        let batch = [&spend[..], &spend[..], &spend[..], &output[..], &output[..]];
        let proofs = run_prove_batch(handle, &batch).unwrap();
        assert_eq!(proofs.len(), 5);
        assert!(proofs.iter().all(|proof| is_a_proof(proof)));
        let _ = take_timings();

        let status = status();
        assert_eq!(status["tripped"], false, "{status}");
        assert_eq!(
            status["cpuRetries"], retries_before,
            "no proof needed the CPU: {status}"
        );
        // The monomial bases of k = 14 are a prefix of those of k = 15, so one
        // table serves both; the Lagrange bases differ per size.
        assert_eq!(status["tables"], 3, "{status}");
        assert_eq!(status["tablePoints"], (2 << 15) + (1 << 14), "{status}");

        // Switched off again, the same registry proves on the CPU.
        set_accelerator(false);
        assert!(is_a_proof(&run_prove(handle, &spend).unwrap()));
        assert_eq!(recorded_gpu_operations(), [0]);

        // A registry configured while the accelerator is on is prepared at once,
        // and closing it frees the tables.
        set_accelerator(true);
        close_registry(handle).unwrap();
        assert_eq!(self::status()["tables"], 0);
        let handle = staged.configure();
        assert_eq!(self::status()["tables"], 3, "{}", self::status());
        close_registry(handle).unwrap();
        return;
    }
    close_registry(handle).unwrap();
}
