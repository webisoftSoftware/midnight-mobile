//! Measures what a circuit's `k` costs the local prover, through the real C ABI.
//!
//! Every subcommand exercises the shipping code path rather than a reimplementation of
//! it: parameters go through `ParamsProver::read` and the registry's `max_k` assertion,
//! and proofs go through `midnight_mobile_local_prover_configure` and
//! `midnight_mobile_local_prover_prove`. So the durations and allocations reported here
//! are the ones a device pays.
//!
//! One `k` per process, because peak memory is the measurement and it has to attribute
//! to a single circuit size. See `tools/circuit-size-bench/README.md` for the ladder and
//! the recorded results.
//!
//! ```text
//! params <file> <k>                       decode one SRS parameter file
//! irk    <file.bzkir>                     print the k an IR requires
//! zswap  <params-dir> <artifacts-dir> spend|output
//!                                         prove a packaged circuit, as calibration
//! prove  <params-dir> <k> <request> [managed-dir]
//!                                         prove a prepared request
//! ```

use std::env::args;
use std::fs::read;
use std::io::Cursor;
use std::process::exit;
use std::time::Instant;

use midnight_mobile_runtime::{
    deterministic_zswap_output_request, deterministic_zswap_spend_request,
};
use midnight_proofs::poly::commitment::Params;
use midnight_transient_crypto::proofs::{ParamsProver, Zkir};
use midnight_zkir::IrSource;
use sha2::{Digest, Sha256};

/// Mirror of the C ABI's parameter descriptor. The FFI module is crate-private, so the
/// layout is restated here and the exports are reached as plain C symbols -- which also
/// keeps this harness honest about using the same boundary a platform bridge uses.
#[repr(C)]
struct ParameterDescriptor {
    k: u32,
    bytes: *const u8,
    bytes_len: usize,
    sha256: *const u8,
}

/// Mirror of the C ABI's circuit descriptor.
#[repr(C)]
struct CircuitDescriptor {
    key_location: *const u8,
    key_location_len: usize,
    prover_key: *const u8,
    prover_key_len: usize,
    prover_key_sha256: *const u8,
    verifier_key: *const u8,
    verifier_key_len: usize,
    verifier_key_sha256: *const u8,
    ir: *const u8,
    ir_len: usize,
    ir_sha256: *const u8,
}

/// Mirror of the C ABI's response.
#[repr(C)]
struct Response {
    bytes: *mut u8,
    bytes_len: usize,
}

unsafe extern "C" {
    fn midnight_mobile_local_prover_configure(
        params: *const ParameterDescriptor,
        params_count: usize,
        circuits: *const CircuitDescriptor,
        circuits_count: usize,
        output_handle: *mut u64,
    ) -> i32;
    fn midnight_mobile_local_prover_prove(
        handle: u64,
        request: *const u8,
        request_len: usize,
        output: *mut Response,
    ) -> i32;
    fn midnight_mobile_local_prover_free(bytes: *mut u8, bytes_len: usize);
}

/// The key location the request builder records, and therefore the one a registered
/// circuit must be filed under for a prepared request to resolve.
const BENCH_KEY_LOCATION: &str = "bench/run";

#[allow(clippy::print_stderr)]
fn main() {
    let arguments: Vec<String> = args().collect();
    match arguments.get(1).map(String::as_str) {
        Some("params") => params(&arguments[2], parsed_k(&arguments[3])),
        Some("irk") => ir_k(&arguments[2]),
        Some("zswap") => zswap(&arguments[2], &arguments[3], &arguments[4]),
        Some("prove") => prove(
            &arguments[2],
            parsed_k(&arguments[3]),
            &arguments[4],
            arguments.get(5).map(String::as_str),
        ),
        _ => {
            eprintln!(
                "usage: circuit-size-bench params <file> <k> | irk <file.bzkir>\n\
                        circuit-size-bench zswap <params-dir> <artifacts-dir> spend|output\n\
                        circuit-size-bench prove <params-dir> <k> <request> [managed-dir]"
            );
            exit(2);
        }
    }
}

#[allow(clippy::expect_used)]
fn parsed_k(value: &str) -> u32 {
    value.parse().expect("k must be an integer")
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Peak resident size in MB from procfs, for Android runs where `/usr/bin/time -l` is
/// unavailable. `None` off Linux; on macOS read `peak memory footprint` from
/// `/usr/bin/time -l` instead, which is what an iOS memory limit is measured against and
/// what stays honest once the host starts reclaiming under pressure.
fn peak_rss_mb() -> Option<u64> {
    let status = read("/proc/self/status").ok()?;
    let text = String::from_utf8_lossy(&status);
    let line = text.lines().find(|line| line.starts_with("VmHWM:"))?;
    Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? / 1024)
}

fn peak_rss_field() -> String {
    match peak_rss_mb() {
        Some(megabytes) => format!(",\"peak_rss_mb\":{megabytes}"),
        None => String::new(),
    }
}

/// Decodes one SRS parameter file exactly as the registry does, including the assertion
/// that rejects a file whose degree does not match its declared `k`.
#[allow(clippy::expect_used, clippy::print_stdout)]
fn params(path: &str, expected_k: u32) {
    let load_start = Instant::now();
    let bytes = read(path).expect("read parameter file");
    let load_ms = load_start.elapsed().as_millis();

    let decode_start = Instant::now();
    let decoded = ParamsProver::read(&bytes[..]).expect("ParamsProver::read");
    let decode_ms = decode_start.elapsed().as_millis();

    let max_k = decoded.0.max_k();
    println!(
        "{{\"k\":{expected_k},\"file_bytes\":{},\"max_k\":{max_k},\"accepted\":{},\
         \"load_ms\":{load_ms},\"decode_ms\":{decode_ms}{}}}",
        bytes.len(),
        max_k == expected_k,
        peak_rss_field(),
    );
    // Held past the report so a peak-memory sample includes the decoded parameters.
    std::hint::black_box(&decoded);
}

/// Prints the `k` an IR requires. This is the value the registry matches a circuit
/// against its parameter set, and the value a caller would need in order to refuse an
/// oversized circuit before proving it.
#[allow(clippy::expect_used, clippy::print_stdout)]
fn ir_k(path: &str) {
    let bytes = read(path).expect("read ir file");
    let ir = IrSource::load_from_tagged(Cursor::new(&bytes[..])).expect("load_from_tagged");
    println!("{{\"file\":\"{path}\",\"ir_k\":{}}}", ir.k());
}

/// Proves one packaged Zswap circuit from the crate's pinned deterministic request.
///
/// This is the calibration subcommand: its numbers can be compared against the recorded
/// device-benchmark baseline, which is what establishes that a synthetic ladder rung of
/// the same `k` is measuring the same work.
#[allow(clippy::expect_used, clippy::panic)]
fn zswap(params_dir: &str, artifacts_dir: &str, circuit: &str) {
    let (k, base, location, request) = match circuit {
        "spend" => (
            15_u32,
            "zswap-spend",
            "midnight/zswap/spend",
            deterministic_zswap_spend_request().expect("spend request"),
        ),
        "output" => (
            14_u32,
            "zswap-output",
            "midnight/zswap/output",
            deterministic_zswap_output_request().expect("output request"),
        ),
        other => panic!("unknown circuit {other}, expected spend or output"),
    };
    let material = Material::load(
        &format!("{artifacts_dir}/{base}.prover"),
        &format!("{artifacts_dir}/{base}.verifier"),
        &format!("{artifacts_dir}/{base}.bzkir"),
    );
    run(params_dir, k, &request, Some((location, &material)));
}

/// Proves a prepared request built by `tools/circuit-size-bench/scripts/build-request.mjs`.
///
/// Without a managed directory the request is expected to carry its own key material
/// inline, which is how the dapp path proves. With one, the circuit is registered in the
/// registry instead -- necessary above roughly 32 MB of prover key, because an inlined
/// request would exceed the ABI's request-size cap.
#[allow(clippy::expect_used)]
fn prove(params_dir: &str, k: u32, request_path: &str, managed: Option<&str>) {
    let request = read(request_path).expect("read request");
    let material = managed.map(|directory| {
        Material::load(
            &format!("{directory}/keys/run.prover"),
            &format!("{directory}/keys/run.verifier"),
            &format!("{directory}/zkir/run.bzkir"),
        )
    });
    run(
        params_dir,
        k,
        &request,
        material
            .as_ref()
            .map(|material| (BENCH_KEY_LOCATION, material)),
    );
}

/// Circuit key material with its hashes, kept alive for the duration of an FFI call.
struct Material {
    prover_key: Vec<u8>,
    verifier_key: Vec<u8>,
    ir: Vec<u8>,
    hashes: [[u8; 32]; 3],
}

impl Material {
    #[allow(clippy::expect_used)]
    fn load(prover_key_path: &str, verifier_key_path: &str, ir_path: &str) -> Self {
        let prover_key = read(prover_key_path).expect("read prover key");
        let verifier_key = read(verifier_key_path).expect("read verifier key");
        let ir = read(ir_path).expect("read ir");
        let hashes = [digest(&prover_key), digest(&verifier_key), digest(&ir)];
        Self {
            prover_key,
            verifier_key,
            ir,
            hashes,
        }
    }

    fn descriptor(&self, location: &str) -> CircuitDescriptor {
        CircuitDescriptor {
            key_location: location.as_ptr(),
            key_location_len: location.len(),
            prover_key: self.prover_key.as_ptr(),
            prover_key_len: self.prover_key.len(),
            prover_key_sha256: self.hashes[0].as_ptr(),
            verifier_key: self.verifier_key.as_ptr(),
            verifier_key_len: self.verifier_key.len(),
            verifier_key_sha256: self.hashes[1].as_ptr(),
            ir: self.ir.as_ptr(),
            ir_len: self.ir.len(),
            ir_sha256: self.hashes[2].as_ptr(),
        }
    }
}

/// Configures a registry holding only the parameter for this `k`, then proves once.
///
/// A single parameter is deliberate: loading the whole packaged set would attribute its
/// memory to every rung of the ladder and blur the per-`k` reading this harness exists
/// to produce.
#[allow(clippy::expect_used, clippy::print_stdout)]
fn run(params_dir: &str, k: u32, request: &[u8], circuit: Option<(&str, &Material)>) {
    let params_bytes = read(format!("{params_dir}/bls_midnight_2p{k}")).expect("read parameters");
    let params_hash = digest(&params_bytes);
    let parameter = ParameterDescriptor {
        k,
        bytes: params_bytes.as_ptr(),
        bytes_len: params_bytes.len(),
        sha256: params_hash.as_ptr(),
    };
    let descriptor = circuit.map(|(location, material)| material.descriptor(location));
    let (circuits, circuits_count) = match descriptor.as_ref() {
        Some(descriptor) => (descriptor as *const CircuitDescriptor, 1),
        None => (std::ptr::null(), 0),
    };

    let mut handle = 0_u64;
    let configure_start = Instant::now();
    // All referenced buffers and hashes outlive this synchronous call.
    // SAFETY: The handle pointer addresses valid writable storage.
    let code = unsafe {
        midnight_mobile_local_prover_configure(&parameter, 1, circuits, circuits_count, &mut handle)
    };
    let configure_ms = configure_start.elapsed().as_millis();
    if code != 0 {
        println!(
            "{{\"k\":{k},\"stage\":\"configure\",\"error_code\":{code},\
             \"configure_ms\":{configure_ms}{}}}",
            peak_rss_field(),
        );
        exit(1);
    }

    let mut response = Response {
        bytes: std::ptr::null_mut(),
        bytes_len: 0,
    };
    let prove_start = Instant::now();
    // The request outlives the call; the successful response is freed once below.
    // SAFETY: The response pointer addresses valid writable storage.
    let code = unsafe {
        midnight_mobile_local_prover_prove(handle, request.as_ptr(), request.len(), &mut response)
    };
    let prove_ms = prove_start.elapsed().as_millis();
    if code != 0 {
        println!(
            "{{\"k\":{k},\"stage\":\"prove\",\"error_code\":{code},\
             \"configure_ms\":{configure_ms},\"prove_ms\":{prove_ms}{}}}",
            peak_rss_field(),
        );
        exit(1);
    }

    println!(
        "{{\"k\":{k},\"stage\":\"ok\",\"configure_ms\":{configure_ms},\"prove_ms\":{prove_ms},\
         \"request_bytes\":{},\"proof_bytes\":{}{}}}",
        request.len(),
        response.bytes_len,
        peak_rss_field(),
    );
    // SAFETY: Pointer and length came from a successful prove call and are freed once.
    unsafe { midnight_mobile_local_prover_free(response.bytes, response.bytes_len) };
}
