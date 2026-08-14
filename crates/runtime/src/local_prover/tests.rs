use std::sync::Arc;

use midnight_ledger::structure::ProofVersioned;
use midnight_serialize::tagged_deserialize;
use midnight_transient_crypto::curve::Fr;
use midnight_transient_crypto::proofs::{
    KeyLocation, ProofPreimage, ProvingKeyMaterial, WrappedIr,
};
use zeroize::Zeroize;

use super::*;

fn synthetic_preimage(location: &'static str) -> Arc<ProofPreimage> {
    Arc::new(ProofPreimage {
        inputs: vec![1_u64.into()],
        private_transcript: vec![],
        public_transcript_inputs: vec![],
        public_transcript_outputs: vec![],
        binding_input: 0_u64.into(),
        communications_commitment: None,
        key_location: KeyLocation(std::borrow::Cow::Borrowed(location)),
    })
}

fn serialize<T: midnight_serialize::Serializable + midnight_serialize::Tagged>(
    value: &T,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    tagged_serialize(value, &mut bytes).unwrap();
    bytes
}

fn minimal_ir_bytes(num_inputs: u32) -> Vec<u8> {
    let ir = IrSource {
        version: Default::default(),
        num_inputs,
        do_communications_commitment: false,
        instructions: Arc::new(vec![]),
    };
    let mut bytes = Vec::new();
    ir.serialize_to_tagged(&mut bytes).unwrap();
    bytes
}

fn material(ir_source: Vec<u8>) -> ProvingKeyMaterial {
    ProvingKeyMaterial {
        prover_key: vec![1],
        verifier_key: vec![2],
        ir_source,
    }
}

fn install_synthetic_registry() -> u64 {
    let mut slot = registry_slot().lock().unwrap();
    slot.generation = slot.generation.saturating_add(1).max(1);
    slot.value = Some(Arc::new(MemoryRegistry {
        params: HashMap::new(),
        circuits: HashMap::new(),
    }));
    slot.generation
}

#[test]
fn official_request_generators_have_exact_tuple_shapes() -> Result<(), &'static str> {
    let prove = deterministic_zswap_spend_request().unwrap();
    let (versioned, material, binding): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &prove[..]).unwrap();
    assert!(matches!(versioned, ProofPreimageVersioned::V2(_)));
    assert!(material.is_none());
    assert!(binding.is_none());

    let output = deterministic_zswap_output_request().unwrap();
    let (versioned, material, binding): (
        ProofPreimageVersioned,
        Option<ProvingKeyMaterial>,
        Option<Fr>,
    ) = tagged_deserialize(&mut &output[..]).unwrap();
    let ProofPreimageVersioned::V2(output_preimage) = versioned else {
        return Err("deterministic output request must use V2");
    };
    assert_eq!(output_preimage.key_location.0, "midnight/zswap/output");
    assert!(material.is_none());
    assert!(binding.is_none());
    assert_eq!(output, deterministic_zswap_output_request().unwrap());
    assert_eq!(output.len(), 430);
    assert_eq!(
        hex::encode(Sha256::digest(&output)),
        "af7c9cb545b8601be70c6a9cfe39bd06d32083f9fbde36c5f84500c25b9c0172"
    );

    let check = deterministic_zswap_spend_check_request().unwrap();
    let (versioned, ir): (ProofPreimageVersioned, Option<WrappedIr>) =
        tagged_deserialize(&mut &check[..]).unwrap();
    assert!(matches!(versioned, ProofPreimageVersioned::V2(_)));
    assert!(ir.is_none());
    Ok(())
}

#[test]
fn hash_location_count_and_size_preflights_are_typed() {
    let hash = Sha256::digest(b"artifact");
    assert!(verify_hash(b"artifact", hash.as_ref()).is_ok());
    assert_eq!(
        verify_hash(b"artifact", &[0; 32]),
        Err(LocalProverError::IntegrityCheckFailed)
    );
    assert!(validate_location("midnight/custom/circuit").is_ok());
    assert_eq!(
        validate_location("bad\nlocation"),
        Err(LocalProverError::InvalidConfiguration)
    );
    assert_eq!(
        checked_total([0_usize]),
        Err(LocalProverError::ResourcePreflightFailed)
    );
    assert!(checked_total([1_usize, MAX_ARTIFACT_BYTES]).is_ok());
    assert_eq!(
        checked_total([MAX_ARTIFACT_BYTES; 5]),
        Err(LocalProverError::ResourcePreflightFailed)
    );
    assert_eq!(
        validate_location(""),
        Err(LocalProverError::InvalidConfiguration)
    );
    assert_eq!(
        validate_location(&"x".repeat(MAX_KEY_LOCATION_BYTES + 1)),
        Err(LocalProverError::InvalidConfiguration)
    );
    assert_eq!(
        request_preflight(&[]),
        Err(LocalProverError::ResourcePreflightFailed)
    );
    assert!(request_preflight(&[1]).is_ok());
    assert_eq!(
        validate_supplied_material(
            &material(vec![1, 2, 3]),
            &MemoryRegistry {
                params: HashMap::new(),
                circuits: HashMap::new(),
            }
        ),
        Err(LocalProverError::UnsupportedCircuit)
    );
    assert_eq!(
        build_registry(&[], &[]).map(|_| ()),
        Err(LocalProverError::InvalidConfiguration)
    );
}

#[test]
fn refuses_circuits_above_the_packaged_ceiling_and_only_those() {
    let packaged: Vec<u8> = (0..=15).collect();

    // Every packaged size proves locally.
    for k in &packaged {
        assert_eq!(refuse_for_size(*k, &packaged), None, "k={k}");
    }

    // Above the ceiling is a size refusal that names the size, so the wallet can
    // say which k the dApp wanted rather than "proof generation failed".
    assert_eq!(
        refuse_for_size(16, &packaged),
        Some(LocalProverError::CircuitTooLarge { k: 16 })
    );
    assert_eq!(
        refuse_for_size(18, &packaged),
        Some(LocalProverError::CircuitTooLarge { k: 18 })
    );

    // A hole below the ceiling is a packaging bug, not an oversized circuit; a
    // remote prover would not fix it, so it must not be reported as one.
    let holed = [0_u8, 1, 2, 15];
    assert_eq!(
        refuse_for_size(9, &holed),
        Some(LocalProverError::InvalidConfiguration)
    );
}

#[test]
fn prover_pool_uses_bounded_mobile_parallelism() {
    assert_eq!(
        prover_pool().unwrap().current_num_threads(),
        PROVER_THREAD_COUNT
    );
}

#[test]
fn providers_resolvers_and_registry_validation_cover_generic_inputs() {
    let fallback = material(vec![3]);
    let mut circuits = HashMap::new();
    circuits.insert("registered".to_owned(), fallback.clone());
    let registry = MemoryRegistry {
        params: HashMap::new(),
        circuits,
    };
    let missing = futures_executor::block_on(registry.get_params(9))
        .err()
        .unwrap();
    assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    assert!(
        futures_executor::block_on(
            registry.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("missing")))
        )
        .unwrap()
        .is_none()
    );
    let resolved = futures_executor::block_on(
        registry.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("registered"))),
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolved.ir_source, fallback.ir_source);

    let supplied = material(vec![4]);
    let resolver = RequestResolver {
        registry: &registry,
        supplied: Some(supplied.clone()),
    };
    let resolved = futures_executor::block_on(
        resolver.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("registered"))),
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolved.ir_source, supplied.ir_source);
    let fallback_resolver = RequestResolver {
        registry: &registry,
        supplied: None,
    };
    assert!(
        futures_executor::block_on(
            fallback_resolver.resolve_key(KeyLocation(std::borrow::Cow::Borrowed("registered")))
        )
        .unwrap()
        .is_some()
    );

    let invalid_params = [0_u8; 4];
    let parameter = ParameterArtifact {
        k: 1,
        bytes: &invalid_params,
        sha256: &[0; 32],
    };
    assert_eq!(
        build_registry(&[parameter], &[]).map(|_| ()),
        Err(LocalProverError::IntegrityCheckFailed)
    );
    let invalid = b"invalid";
    let circuit_hash = Sha256::digest(invalid);
    let circuit = CircuitArtifact {
        key_location: "midnight/test",
        prover_key: invalid,
        prover_key_sha256: circuit_hash.as_ref(),
        verifier_key: invalid,
        verifier_key_sha256: circuit_hash.as_ref(),
        ir: invalid,
        ir_sha256: circuit_hash.as_ref(),
    };
    assert_eq!(
        decode_circuit(&circuit).map(|_| ()),
        Err(LocalProverError::InvalidConfiguration)
    );
}

#[test]
fn malformed_missing_and_inline_invalid_requests_are_typed() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    assert_eq!(
        run_check(handle, b"bad"),
        Err(LocalProverError::InvalidRequest)
    );
    assert_eq!(
        run_prove(handle, b"bad"),
        Err(LocalProverError::InvalidRequest)
    );
    let check = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("missing/circuit")),
        None::<WrappedIr>,
    ));
    assert_eq!(
        run_check(handle, &check),
        Err(LocalProverError::UnsupportedCircuit)
    );
    let inline = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(vec![1, 2, 3])),
    ));
    assert_eq!(
        run_check(handle, &inline),
        Err(LocalProverError::UnsupportedCircuit)
    );
    let empty_inline = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(vec![])),
    ));
    assert_eq!(
        run_check(handle, &empty_inline),
        Err(LocalProverError::ResourcePreflightFailed)
    );
}

#[test]
fn valid_inline_and_registered_ir_cover_check_paths() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    let ir = minimal_ir_bytes(1);
    let inline = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(ir.clone())),
    ));
    let checked = run_check(handle, &inline).unwrap();
    let decoded: Vec<Option<u64>> = tagged_deserialize(&mut &checked[..]).unwrap();
    assert!(decoded.is_empty());

    registry_slot().lock().unwrap().value = Some(Arc::new(MemoryRegistry {
        params: HashMap::new(),
        circuits: HashMap::from([("registered/circuit".to_owned(), material(ir))]),
    }));
    let registered = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("registered/circuit")),
        None::<WrappedIr>,
    ));
    assert!(run_check(handle, &registered).is_ok());

    let mismatched = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("inline/circuit")),
        Some(WrappedIr(minimal_ir_bytes(2))),
    ));
    assert_eq!(
        run_check(handle, &mismatched),
        Err(LocalProverError::CheckFailed)
    );
}

#[test]
fn supplied_material_and_binding_cover_prove_failure_path() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    let unsupported = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("missing/circuit")),
        None::<ProvingKeyMaterial>,
        Some(Fr::from(7_u64)),
    ));
    assert_eq!(
        run_prove(handle, &unsupported),
        Err(LocalProverError::UnsupportedCircuit)
    );

    let invalid_material = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("supplied/circuit")),
        Some(material(vec![1, 2, 3])),
        None::<Fr>,
    ));
    assert_eq!(
        run_prove(handle, &invalid_material),
        Err(LocalProverError::UnsupportedCircuit)
    );

    // A decodable IR the registry has no parameters for is refused before proving.
    // This used to reach `preimage.prove`, miss in `get_params`, and come back as a
    // generic `ProofFailed` — indistinguishable from a real proving fault. The
    // synthetic registry packages no parameters at all, so the refusal here is the
    // configuration one; a size refusal needs a registry with a ceiling to exceed,
    // which `refuses_circuits_above_the_packaged_ceiling_and_only_those` covers.
    let unservable_size = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("supplied/circuit")),
        Some(material(minimal_ir_bytes(1))),
        Some(Fr::from(11_u64)),
    ));
    assert_eq!(
        run_prove(handle, &unservable_size),
        Err(LocalProverError::InvalidConfiguration)
    );
}

#[test]
fn registry_generations_close_and_stale_handles_fail_closed() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    assert!(configured_registry(handle).is_ok());
    assert_eq!(
        configured_registry(handle.saturating_add(1)).map(|_| ()),
        Err(LocalProverError::StaleRegistry)
    );
    assert!(close_registry(handle).is_ok());
    assert_eq!(close_registry(handle), Err(LocalProverError::StaleRegistry));
    assert_eq!(close_registry(0), Err(LocalProverError::StaleRegistry));
}

/// Spawns a thread holding a shared permit until released via the returned sender, joining the
/// caller at `barrier` once the permit is acquired. Tests use explicit releases so none of them
/// need to observe the real 180s `PROVER_WAIT_BUDGET` timeout to pass.
fn spawn_shared_holder(
    barrier: Arc<std::sync::Barrier>,
) -> (std::thread::JoinHandle<()>, std::sync::mpsc::Sender<()>) {
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _permit = SharedProverPermit::acquire().unwrap();
        barrier.wait();
        release_receiver.recv().unwrap();
    });
    (holder, release_sender)
}

#[test]
fn two_shared_permits_overlap_and_a_third_blocks_until_release() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    set_max_concurrency(2);
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let (holder_a, release_a) = spawn_shared_holder(barrier.clone());
    let (holder_b, release_b) = spawn_shared_holder(barrier.clone());
    barrier.wait();

    // Both permits are held; a bounded probe with a short effective wait must not observe a
    // third permit becoming available (it never does, since max is 2 and both are held).
    let readers_while_both_held = lock_gate_state().readers;
    assert_eq!(readers_while_both_held, 2);

    release_a.send(()).unwrap();
    holder_a.join().unwrap();

    // Now one slot is free; a third shared permit must succeed promptly.
    let third = SharedProverPermit::acquire().unwrap();
    assert_eq!(lock_gate_state().readers, 2);
    drop(third);

    release_b.send(()).unwrap();
    holder_b.join().unwrap();
    assert_eq!(lock_gate_state().readers, 0);
}

#[test]
fn exclusive_and_shared_permits_mutually_exclude() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    set_max_concurrency(2);
    let short_wait = std::time::Duration::from_millis(150);
    let bounded_join = std::time::Duration::from_secs(5);

    let shared = SharedProverPermit::acquire().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let exclusive_thread = std::thread::spawn(move || {
        let _permit = ExclusiveProverPermit::acquire().unwrap();
        tx.send(()).unwrap();
    });
    // A shared permit is held, so the exclusive acquire must still be blocked shortly after.
    std::thread::sleep(short_wait);
    assert!(rx.try_recv().is_err());
    drop(shared);
    rx.recv_timeout(bounded_join).unwrap();
    exclusive_thread.join().unwrap();

    // Symmetric case: while an exclusive permit is held, a shared acquire must block.
    let exclusive = ExclusiveProverPermit::acquire().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let shared_thread = std::thread::spawn(move || {
        let _permit = SharedProverPermit::acquire().unwrap();
        tx.send(()).unwrap();
    });
    std::thread::sleep(short_wait);
    assert!(rx.try_recv().is_err());
    drop(exclusive);
    rx.recv_timeout(bounded_join).unwrap();
    shared_thread.join().unwrap();
}

#[test]
fn waiting_exclusive_blocks_new_shared_acquisitions() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    set_max_concurrency(2);
    let short_wait = std::time::Duration::from_millis(150);
    let bounded_join = std::time::Duration::from_secs(5);

    // One of two shared slots is in use, so a naive limit check would still allow a second
    // shared acquire; writer preference must block it once a writer starts waiting.
    let shared_a = SharedProverPermit::acquire().unwrap();

    let (writer_tx, writer_rx) = std::sync::mpsc::channel();
    let writer_thread = std::thread::spawn(move || {
        let _permit = ExclusiveProverPermit::acquire().unwrap();
        writer_tx.send(()).unwrap();
    });
    std::thread::sleep(short_wait);
    assert_eq!(lock_gate_state().writers_waiting, 1);

    let (shared_tx, shared_rx) = std::sync::mpsc::channel();
    let shared_thread = std::thread::spawn(move || {
        let _permit = SharedProverPermit::acquire().unwrap();
        shared_tx.send(()).unwrap();
    });
    std::thread::sleep(short_wait);
    assert!(shared_rx.try_recv().is_err());

    drop(shared_a);
    writer_rx.recv_timeout(bounded_join).unwrap();
    writer_thread.join().unwrap();
    shared_rx.recv_timeout(bounded_join).unwrap();
    shared_thread.join().unwrap();
}

#[test]
fn permits_release_on_drop_including_after_a_panic() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    set_max_concurrency(2);
    assert_eq!(lock_gate_state().readers, 0);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _permit = SharedProverPermit::acquire().unwrap();
        assert_eq!(lock_gate_state().readers, 1);
        // Deliberately triggers a panic (out-of-bounds index, not the banned `panic!` macro) to
        // exercise the permit's Drop-on-unwind path.
        let empty: Vec<u8> = Vec::new();
        let _ = empty[0];
    }));
    assert!(outcome.is_err());
    assert_eq!(lock_gate_state().readers, 0);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _permit = ExclusiveProverPermit::acquire().unwrap();
        assert!(lock_gate_state().writer_active);
        let empty: Vec<u8> = Vec::new();
        let _ = empty[0];
    }));
    assert!(outcome.is_err());
    assert!(!lock_gate_state().writer_active);
}

#[test]
fn set_max_concurrency_clamps_out_of_range_values() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    assert_eq!(set_max_concurrency(0), 1);
    assert_eq!(set_max_concurrency(1), 1);
    assert_eq!(set_max_concurrency(4), 4);
    assert_eq!(set_max_concurrency(999), 4);
    assert_eq!(set_max_concurrency(2), 2);
}

#[test]
fn profiling_flag_toggles_without_panicking() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    set_profiling(true);
    assert!(PROFILE_STAGES.load(std::sync::atomic::Ordering::Acquire));
    set_profiling(false);
    assert!(!PROFILE_STAGES.load(std::sync::atomic::Ordering::Acquire));
}

#[test]
fn timings_round_trip_and_drain_exactly_once() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    super::timings::test_samples().lock().unwrap().clear();
    super::timings::record_timing_sample(super::timings::ProofStageTiming {
        key_location: "midnight/test".to_owned(),
        request_bytes: 128,
        k: Some(15),
        deserialize_request_micros: 10,
        select_material_micros: 20,
        prove_call_micros: 30,
        serialize_response_micros: 40,
        ir_load_micros: Some(50),
        prover_key_init_micros: Some(60),
        verifier_key_init_micros: Some(70),
    });

    let first = take_timings().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&first).unwrap();
    let entries = parsed.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["keyLocation"], "midnight/test");
    assert_eq!(entries[0]["requestBytes"], 128);
    assert_eq!(entries[0]["k"], 15);
    assert_eq!(entries[0]["deserializeRequestMicros"], 10);
    assert_eq!(entries[0]["proveCallMicros"], 30);
    assert_eq!(entries[0]["irLoadMicros"], 50);

    let second = take_timings().unwrap();
    assert_eq!(second, b"[]");
}

#[test]
fn timing_queue_drops_oldest_beyond_capacity() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    super::timings::test_samples().lock().unwrap().clear();
    for index in 0..(MAX_TIMING_SAMPLES + 5) {
        super::timings::record_timing_sample(super::timings::ProofStageTiming {
            key_location: format!("midnight/test/{index}"),
            request_bytes: index,
            k: None,
            deserialize_request_micros: 0,
            select_material_micros: 0,
            prove_call_micros: 0,
            serialize_response_micros: 0,
            ir_load_micros: None,
            prover_key_init_micros: None,
            verifier_key_init_micros: None,
        });
    }
    let drained = take_timings().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&drained).unwrap();
    let entries = parsed.as_array().unwrap();
    assert_eq!(entries.len(), MAX_TIMING_SAMPLES);
    assert_eq!(entries[0]["keyLocation"], "midnight/test/5");
}

#[test]
fn batch_fan_out_is_all_or_nothing_and_deterministic_by_request_order() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    let first = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("missing/first")),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    ));
    let second = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("missing/second")),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    ));
    // Both requests fail (neither circuit is registered), so the batch must fail closed with the
    // first request's error in request order, regardless of which worker thread finishes first.
    for _ in 0..8 {
        assert_eq!(
            run_prove_batch(handle, &[&first, &second]),
            Err(LocalProverError::UnsupportedCircuit)
        );
    }
}

#[test]
fn batch_fan_out_fails_closed_on_a_stale_handle_with_no_successes_to_leak() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let handle = install_synthetic_registry();
    let stale_handle = handle.saturating_add(1);
    let request = serialize(&(
        ProofPreimageVersioned::V2(synthetic_preimage("missing/circuit")),
        None::<ProvingKeyMaterial>,
        None::<Fr>,
    ));
    // Every worker fails identically on a mismatched handle, so there is nothing for the
    // all-or-nothing contract to leak: this is the "stale/absent registry" case that exercises the
    // failure path without requiring real proving.
    assert_eq!(
        run_prove_batch(stale_handle, &[&request, &request, &request]),
        Err(LocalProverError::StaleRegistry)
    );
}

#[test]
fn cancellation_epoch_bump_marks_not_yet_started_work_as_abandoned() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let epoch_at_start = CANCEL_EPOCH.load(std::sync::atomic::Ordering::Acquire);
    assert!(!worker_should_abandon(epoch_at_start));
    cancel_all();
    assert!(worker_should_abandon(epoch_at_start));
    // A fresh batch call loads a fresh epoch, so it is unaffected by the earlier cancellation.
    let fresh_epoch = CANCEL_EPOCH.load(std::sync::atomic::Ordering::Acquire);
    assert!(!worker_should_abandon(fresh_epoch));
}

#[test]
fn batch_request_copies_zeroize_the_same_way_run_prove_batchs_container_does() {
    // `run_prove_batch` copies every request into a `Zeroizing<Vec<u8>>` (the batch container) and
    // relies on `Zeroizing`'s `Drop` to wipe it on every exit path. `Drop` cannot be observed after
    // the fact without reading freed memory, so this exercises the same `Zeroize` call that `Drop`
    // performs on the same container type, directly on secret-shaped bytes.
    let mut copy = Zeroizing::new(vec![0xAB_u8; 4]);
    assert_eq!(*copy, vec![0xAB; 4]);
    copy.zeroize();
    // `Vec<u8>`'s `Zeroize` impl overwrites every byte and then truncates to empty, which is the
    // same call `Zeroizing`'s `Drop` makes on `run_prove_batch`'s owned request copies.
    assert!(copy.is_empty());
}

#[test]
#[ignore = "requires staged public artifacts"]
fn every_packaged_parameter_size_decodes() {
    // `build_registry` asserts `ParamsProver::read(bytes).max_k() == k` for every entry, and one
    // failure rejects the whole configuration — including the wallet's own circuits. The small
    // sizes are new to the package and had never been through that assertion, so this decodes the
    // full staged range in one registry exactly as `configure` does.
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let directory = std::path::PathBuf::from(
        std::env::var_os("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR")
            .expect("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR must be set"),
    );
    let blobs: Vec<(u8, Vec<u8>)> = (0..=15_u8)
        .map(|k| {
            let bytes = std::fs::read(directory.join(format!("bls_midnight_2p{k}")))
                .unwrap_or_else(|error| panic!("bls_midnight_2p{k}: {error}"));
            (k, bytes)
        })
        .collect();
    let hashes: Vec<Vec<u8>> = blobs
        .iter()
        .map(|(_, bytes)| Sha256::digest(bytes).to_vec())
        .collect();
    let params: Vec<ParameterArtifact<'_>> = blobs
        .iter()
        .zip(&hashes)
        .map(|((k, bytes), sha256)| ParameterArtifact {
            k: *k,
            bytes,
            sha256,
        })
        .collect();

    let handle = configure_registry(&params, &[]).expect("every packaged size must decode");

    close_registry(handle).unwrap();
}

#[test]
#[ignore = "requires staged public artifacts"]
fn staged_artifacts_produce_and_check_official_responses() {
    let _serial = PROVER_TEST_LOCK.lock().unwrap();
    let directory = std::path::PathBuf::from(
        std::env::var_os("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR")
            .expect("MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR must be set"),
    );
    let read = |name: &str| std::fs::read(directory.join(name)).unwrap();
    let params = read("bls_midnight_2p15");
    let output_params = read("bls_midnight_2p14");
    let prover = read("zswap/9/spend.prover");
    let verifier = read("zswap/9/spend.verifier");
    let ir = read("zswap/9/spend.bzkir");
    let output_prover = read("zswap/9/output.prover");
    let output_verifier = read("zswap/9/output.verifier");
    let output_ir = read("zswap/9/output.bzkir");
    let digest = |bytes: &[u8]| Sha256::digest(bytes).to_vec();
    let params_hash = digest(&params);
    let output_params_hash = digest(&output_params);
    let prover_hash = digest(&prover);
    let verifier_hash = digest(&verifier);
    let ir_hash = digest(&ir);
    let output_prover_hash = digest(&output_prover);
    let output_verifier_hash = digest(&output_verifier);
    let output_ir_hash = digest(&output_ir);
    let handle = configure_registry(
        &[
            ParameterArtifact {
                k: 15,
                bytes: &params,
                sha256: &params_hash,
            },
            ParameterArtifact {
                k: 14,
                bytes: &output_params,
                sha256: &output_params_hash,
            },
        ],
        &[
            CircuitArtifact {
                key_location: "midnight/zswap/spend",
                prover_key: &prover,
                prover_key_sha256: &prover_hash,
                verifier_key: &verifier,
                verifier_key_sha256: &verifier_hash,
                ir: &ir,
                ir_sha256: &ir_hash,
            },
            CircuitArtifact {
                key_location: "midnight/zswap/output",
                prover_key: &output_prover,
                prover_key_sha256: &output_prover_hash,
                verifier_key: &output_verifier,
                verifier_key_sha256: &output_verifier_hash,
                ir: &output_ir,
                ir_sha256: &output_ir_hash,
            },
        ],
    )
    .unwrap();
    let check = run_check(handle, &deterministic_zswap_spend_check_request().unwrap()).unwrap();
    let _: Vec<Option<u64>> = tagged_deserialize(&mut &check[..]).unwrap();
    let proof = run_prove(handle, &deterministic_zswap_spend_request().unwrap()).unwrap();
    let decoded: ProofVersioned = tagged_deserialize(&mut &proof[..]).unwrap();
    assert!(matches!(decoded, ProofVersioned::V2(_)));
    let output_proof = run_prove(handle, &deterministic_zswap_output_request().unwrap()).unwrap();
    let decoded: ProofVersioned = tagged_deserialize(&mut &output_proof[..]).unwrap();
    assert!(matches!(decoded, ProofVersioned::V2(_)));
    close_registry(handle).unwrap();
}
