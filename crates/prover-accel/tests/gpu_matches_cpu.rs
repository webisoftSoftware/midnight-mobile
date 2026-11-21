//! The accelerated prover against the stock CPU prover, for both prover
//! builds the local prover links.
//!
//! Every test computes a value twice in one process: with the accelerator on
//! and with it switched off, where the hooks decline and the stock code runs.
//! The two must be equal, and the counters must show that the first run used
//! the device.
//!
//! These tests need a Metal device. On a machine without one they pass
//! without checking anything, unless `MIDNIGHT_MOBILE_REQUIRE_GPU=1` is set,
//! which turns a missing device into a failure.
#![cfg(any(target_os = "macos", target_os = "ios"))]

use std::sync::{Mutex, MutexGuard, PoisonError};

use midnight_mobile_prover_accel as accel;

/// The accelerator's switch and thresholds are process-wide.
static SERIAL: Mutex<()> = Mutex::new(());

/// Operations of at least `2^FORCED_MIN_LOG` elements go to the device in
/// these tests, so small circuits exercise it.
const FORCED_MIN_LOG: u32 = 8;

/// Takes the process-wide lock and installs the accelerator. `None`: no
/// device, and the caller returns without checking anything.
fn device() -> Option<MutexGuard<'static, ()>> {
    let guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let availability = accel::install();
    if availability != accel::Availability::Active {
        assert!(
            std::env::var("MIDNIGHT_MOBILE_REQUIRE_GPU").as_deref() != Ok("1"),
            "a GPU is required but the accelerator is {}",
            availability.name()
        );
        return None;
    }
    accel::set_enabled(true);
    accel::override_min_log_size(Some(FORCED_MIN_LOG));
    Some(guard)
}

/// Runs `f` with the accelerator switched off.
fn on_cpu<T>(f: impl FnOnce() -> T) -> T {
    accel::set_enabled(false);
    let value = f();
    accel::set_enabled(true);
    value
}

macro_rules! prover_suite {
    (@prove rng_first, $proofs:ident, $params:expr, $pk:expr, $circuit:expr, $instances:expr,
     $rng:expr, $transcript:expr) => {
        $proofs::plonk::create_proof::<Fq, Kzg, _, _>(
            $params, $pk, $circuit, 0, &[$instances], $rng, $transcript,
        )
    };
    (@prove transcript_first, $proofs:ident, $params:expr, $pk:expr, $circuit:expr,
     $instances:expr, $rng:expr, $transcript:expr) => {
        $proofs::plonk::create_proof::<Fq, Kzg, _, _>(
            $params, $pk, $circuit, 0, &[$instances], $transcript, $rng,
        )
    };
    ($module:ident, $proofs:ident, $curves:ident, $order:ident) => {
        mod $module {
            use blake2b_simd::State;
            use ff::Field;
            use group::Curve;
            use rand_chacha::ChaCha20Rng;
            use rand_core::SeedableRng;
            use $curves::{Bls12, Fq};
            use $proofs::circuit::{Layouter, SimpleFloorPlanner, Value};
            use $proofs::plonk::{
                Advice, Circuit, Column, ConstraintSystem, Constraints, Error, Fixed, Instance,
                ProvingKey, keygen_pk, keygen_vk_with_k, prepare,
            };
            use $proofs::poly::commitment::{Guard, PolynomialCommitmentScheme};
            use $proofs::poly::kzg::KZGCommitmentScheme;
            use $proofs::poly::kzg::params::ParamsKZG;
            use $proofs::poly::{EvaluationDomain, Rotation};
            use $proofs::transcript::{CircuitTranscript, Transcript};
            use $proofs::utils::SerdeFormat;

            use super::{accel, device, on_cpu};

            type Kzg = KZGCommitmentScheme<Bls12>;

            fn scalars(len: usize, seed: u64) -> Vec<Fq> {
                let mut rng = ChaCha20Rng::seed_from_u64(seed);
                (0..len).map(|_| Fq::random(&mut rng)).collect()
            }

            /// Parameters as `unsafe_setup` leaves them: Jacobian points that
            /// are not normalized.
            fn raw_params(k: u32, seed: u64) -> ParamsKZG<Bls12> {
                ParamsKZG::unsafe_setup(k, ChaCha20Rng::seed_from_u64(seed))
            }

            /// Parameters as the wallet loads them: read from bytes, so every
            /// point is normalized. Each call allocates new vectors.
            fn loaded_params(k: u32, seed: u64) -> ParamsKZG<Bls12> {
                let mut bytes = Vec::new();
                raw_params(k, seed).write_custom(&mut bytes, SerdeFormat::RawBytes).unwrap();
                ParamsKZG::read_custom(&mut &bytes[..], SerdeFormat::RawBytes).unwrap()
            }

            fn commit_both_ways(params: &ParamsKZG<Bls12>, k: u32, seed: u64) {
                let domain = EvaluationDomain::<Fq>::new(3, k);
                let coeff = domain.coeff_from_vec(scalars(1 << k, seed));
                let lagrange = domain.lagrange_from_vec(scalars(1 << k, seed + 1));

                let before = accel::counters();
                let gpu = (
                    Kzg::commit(params, &coeff).to_affine(),
                    Kzg::commit_lagrange(params, &lagrange).to_affine(),
                );
                let used = accel::counters().since(&before);
                assert_eq!(used.msm_gpu, 2, "both commitments on the device: {used:?}");
                assert_eq!(used.msm_cpu, 0, "{used:?}");

                let cpu = on_cpu(|| {
                    (
                        Kzg::commit(params, &coeff).to_affine(),
                        Kzg::commit_lagrange(params, &lagrange).to_affine(),
                    )
                });
                assert!(gpu == cpu, "the device commitments differ from the stock ones");
            }

            #[test]
            fn commitments_match_stock_on_loaded_parameters() {
                let Some(_guard) = device() else { return };
                for k in [9, 12, 14] {
                    commit_both_ways(&loaded_params(k, 40 + u64::from(k)), k, 7);
                }
            }

            #[test]
            fn commitments_match_stock_on_unnormalized_parameters() {
                let Some(_guard) = device() else { return };
                commit_both_ways(&raw_params(11, 3), 11, 9);
            }

            #[test]
            fn reloaded_parameters_reuse_one_table_per_base_vector() {
                let Some(_guard) = device() else { return };
                accel::release_tables();
                // The wallet may load the same parameters again for every
                // proof: the vectors move, their contents do not.
                for _ in 0..3 {
                    commit_both_ways(&loaded_params(10, 77), 10, 5);
                }
                let counters = accel::counters();
                assert_eq!(counters.tables, 2, "one table each for g and g_lagrange");
                assert_eq!(counters.table_points, 2 << 10);
                accel::release_tables();
                assert_eq!(accel::counters().tables, 0);
                // A released table is rebuilt on demand.
                commit_both_ways(&loaded_params(10, 77), 10, 6);
            }

            #[test]
            fn warming_prepares_the_tables_before_the_first_commitment() {
                let Some(_guard) = device() else { return };
                accel::release_tables();
                let params = loaded_params(10, 71);
                $proofs::gpu::warm(&params);
                assert_eq!(accel::counters().tables, 2, "g and g_lagrange");
                // The commitments find the warmed tables and build no other.
                commit_both_ways(&params, 10, 2);
                assert_eq!(accel::counters().tables, 2);

                // Switched off, warming prepares nothing.
                accel::release_tables();
                super::on_cpu(|| $proofs::gpu::warm(&params));
                assert_eq!(accel::counters().tables, 0);
            }

            #[test]
            fn trimming_memory_frees_the_tables_and_proving_continues() {
                let Some(_guard) = device() else { return };
                let params = loaded_params(10, 61);
                commit_both_ways(&params, 10, 3);
                assert!(accel::counters().tables > 0);
                // What the host calls on a memory warning.
                accel::trim_memory();
                assert_eq!(accel::counters().tables, 0);
                assert_eq!(accel::counters().table_points, 0);
                commit_both_ways(&params, 10, 4);
                assert_eq!(accel::counters().tables, 2, "rebuilt on demand");
            }

            #[test]
            fn different_parameters_of_one_size_never_share_a_table() {
                let Some(_guard) = device() else { return };
                let (first, second) = (loaded_params(10, 1), loaded_params(10, 2));
                for round in 0..2 {
                    commit_both_ways(&first, 10, 20 + round);
                    commit_both_ways(&second, 10, 30 + round);
                }
            }

            #[test]
            fn zero_and_sparse_polynomials_match_stock() {
                let Some(_guard) = device() else { return };
                let k = 10;
                let params = loaded_params(k, 55);
                let domain = EvaluationDomain::<Fq>::new(3, k);
                let mut sparse = vec![Fq::ZERO; 1 << k];
                sparse[0] = Fq::ONE;
                sparse[(1 << k) - 1] = -Fq::ONE;
                for values in [vec![Fq::ZERO; 1 << k], sparse, vec![-Fq::ONE; 1 << k]] {
                    let poly = domain.coeff_from_vec(values);
                    // The device may decline a degenerate input; either way
                    // the commitment is the stock one.
                    let offered = Kzg::commit(&params, &poly).to_affine();
                    let cpu = on_cpu(|| Kzg::commit(&params, &poly).to_affine());
                    assert!(offered == cpu);
                }
            }

            #[test]
            fn domain_transforms_match_stock() {
                let Some(_guard) = device() else { return };
                for k in [8, 11, 13] {
                    let domain = EvaluationDomain::<Fq>::new(5, k);
                    let values = scalars(1 << k, 100 + u64::from(k));
                    let run = || {
                        let lagrange =
                            domain.coeff_to_lagrange(domain.coeff_from_vec(values.clone()));
                        let coeff =
                            domain.lagrange_to_coeff(domain.lagrange_from_vec(values.clone()));
                        let extended =
                            domain.coeff_to_extended(domain.coeff_from_vec(values.clone()));
                        let back = domain.extended_to_coeff(extended.clone());
                        (lagrange.to_vec(), coeff.to_vec(), extended.to_vec(), back)
                    };
                    let before = accel::counters();
                    let gpu = run();
                    let used = accel::counters().since(&before);
                    assert_eq!((used.fft_gpu, used.fft_cpu), (2, 0), "k={k}: {used:?}");
                    assert_eq!((used.ifft_gpu, used.ifft_cpu), (2, 0), "k={k}: {used:?}");
                    let cpu = on_cpu(run);
                    assert!(gpu == cpu, "k={k}: a device transform differs from the stock one");
                    assert!(gpu.3[..1 << k] == values[..], "k={k}: the round trip");
                }
            }

            #[test]
            fn a_switched_off_accelerator_runs_nothing_on_the_device() {
                let Some(_guard) = device() else { return };
                let params = loaded_params(10, 8);
                let before = accel::counters();
                on_cpu(|| commit_cpu_only(&params));
                let used = accel::counters().since(&before);
                assert_eq!(used.gpu_operations(), 0, "{used:?}");
                assert_eq!(used.msm_cpu, 1, "{used:?}");
            }

            fn commit_cpu_only(params: &ParamsKZG<Bls12>) {
                let domain = EvaluationDomain::<Fq>::new(3, 10);
                let _ = Kzg::commit(params, &domain.coeff_from_vec(scalars(1 << 10, 1)));
            }

            #[derive(Clone, Copy)]
            struct Config {
                a: Column<Advice>,
                b: Column<Advice>,
                c: Column<Advice>,
                q_a: Column<Fixed>,
                q_ab: Column<Fixed>,
                q_c: Column<Fixed>,
                instance: Column<Instance>,
            }

            /// `rows` multiplication gates chained by copy constraints, plus
            /// one public input.
            #[derive(Clone, Default)]
            struct Chain {
                start: Fq,
                rows: usize,
            }

            impl Chain {
                /// The public input: `start^(2^rows)`.
                fn output(&self) -> Fq {
                    (0..self.rows).fold(self.start, |x, _| x * x)
                }
            }

            impl Circuit<Fq> for Chain {
                type Config = Config;
                type FloorPlanner = SimpleFloorPlanner;
                type Params = ();

                fn without_witnesses(&self) -> Self {
                    Self { start: Fq::ZERO, rows: self.rows }
                }

                fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
                    let [a, b, c] = [(); 3].map(|()| meta.advice_column());
                    let [q_a, q_ab, q_c] = [(); 3].map(|()| meta.fixed_column());
                    let instance = meta.instance_column();
                    for column in [a, b, c] {
                        meta.enable_equality(column);
                    }
                    meta.enable_equality(instance);
                    meta.create_gate("q_a*a + q_ab*a*b + q_c*c = 0", |meta| {
                        let [a, b, c] =
                            [a, b, c].map(|column| meta.query_advice(column, Rotation::cur()));
                        let [q_a, q_ab, q_c] =
                            [q_a, q_ab, q_c].map(|column| meta.query_fixed(column, Rotation::cur()));
                        Constraints::without_selector(vec![(
                            "arithmetic",
                            q_a * &a + q_ab * a * b + q_c * c,
                        )])
                    });
                    Config { a, b, c, q_a, q_ab, q_c, instance }
                }

                fn synthesize(
                    &self,
                    config: Config,
                    mut layouter: impl Layouter<Fq>,
                ) -> Result<(), Error> {
                    let last = layouter.assign_region(
                        || "chain",
                        |mut region| {
                            let mut value = Value::known(self.start);
                            let mut previous = None;
                            for row in 0..self.rows {
                                let a = region.assign_advice(|| "a", config.a, row, || value)?;
                                let b = region.assign_advice(|| "b", config.b, row, || value)?;
                                region.constrain_equal(a.cell(), b.cell())?;
                                if let Some(previous) = previous {
                                    region.constrain_equal(previous, a.cell())?;
                                }
                                value = value.map(|x| x * x);
                                let c = region.assign_advice(|| "c", config.c, row, || value)?;
                                region.assign_fixed(
                                    || "q_ab",
                                    config.q_ab,
                                    row,
                                    || Value::known(Fq::ONE),
                                )?;
                                region.assign_fixed(
                                    || "q_c",
                                    config.q_c,
                                    row,
                                    || Value::known(-Fq::ONE),
                                )?;
                                region.assign_fixed(
                                    || "q_a",
                                    config.q_a,
                                    row,
                                    || Value::known(Fq::ZERO),
                                )?;
                                previous = Some(c.cell());
                            }
                            previous.ok_or_else(|| Error::Synthesis("an empty chain".into()))
                        },
                    )?;
                    layouter.constrain_instance(last, config.instance, 0)
                }
            }

            fn keys(params: &ParamsKZG<Bls12>, circuit: &Chain, k: u32) -> ProvingKey<Fq, Kzg> {
                let vk = keygen_vk_with_k::<_, Kzg, _>(params, circuit, k).unwrap();
                keygen_pk(vk, circuit).unwrap()
            }

            fn prove(
                params: &ParamsKZG<Bls12>,
                pk: &ProvingKey<Fq, Kzg>,
                circuit: &Chain,
                seed: u64,
            ) -> Vec<u8> {
                let _request = accel::Request::begin();
                let output = [circuit.output()];
                let instances: &[&[Fq]] = &[&output];
                let mut transcript = CircuitTranscript::<State>::init();
                prover_suite!(
                    @prove $order,
                    $proofs,
                    params,
                    pk,
                    std::slice::from_ref(circuit),
                    instances,
                    ChaCha20Rng::seed_from_u64(seed),
                    &mut transcript
                )
                .unwrap();
                transcript.finalize()
            }

            fn verifies(
                params: &ParamsKZG<Bls12>,
                pk: &ProvingKey<Fq, Kzg>,
                circuit: &Chain,
                proof: &[u8],
            ) -> bool {
                let output = [circuit.output()];
                let instances: &[&[Fq]] = &[&output];
                let mut transcript = CircuitTranscript::<State>::init_from_bytes(proof);
                prepare::<Fq, Kzg, _>(pk.get_vk(), &[&[]], &[instances], &mut transcript)
                    .is_ok_and(|guard| guard.verify(&params.verifier_params()).is_ok())
            }

            #[test]
            fn accelerated_proofs_are_the_stock_proofs() {
                let Some(_guard) = device() else { return };
                for k in [9u32, 12] {
                    let params = loaded_params(k, 500 + u64::from(k));
                    let circuit =
                        Chain { start: Fq::from(3u64), rows: (1usize << k) - 64 };
                    let pk = keys(&params, &circuit, k);

                    let before = accel::counters();
                    let gpu = prove(&params, &pk, &circuit, 11);
                    let used = accel::counters().since(&before);
                    assert!(used.msm_gpu > 0, "k={k}: no commitment on the device: {used:?}");
                    assert!(
                        used.fft_gpu + used.ifft_gpu > 0,
                        "k={k}: no transform on the device: {used:?}"
                    );

                    let cpu = on_cpu(|| prove(&params, &pk, &circuit, 11));
                    assert!(gpu == cpu, "k={k}: the accelerated proof is not the stock proof");
                    assert!(verifies(&params, &pk, &circuit, &gpu), "k={k}: the proof verifies");

                    let mut tampered = gpu.clone();
                    let middle = tampered.len() / 2;
                    tampered[middle] ^= 1;
                    assert!(!verifies(&params, &pk, &circuit, &tampered), "k={k}");
                }
            }

            #[test]
            fn concurrent_accelerated_proofs_are_the_stock_proofs() {
                let Some(_guard) = device() else { return };
                let k = 10u32;
                let params = loaded_params(k, 900);
                let circuit = Chain { start: Fq::from(5u64), rows: (1usize << k) - 64 };
                let pk = keys(&params, &circuit, k);
                let expected: Vec<Vec<u8>> =
                    on_cpu(|| (0..4).map(|seed| prove(&params, &pk, &circuit, seed)).collect());

                let before = accel::counters();
                let proofs: Vec<Vec<u8>> = std::thread::scope(|scope| {
                    let handles: Vec<_> = (0..4)
                        .map(|seed| {
                            let (params, pk, circuit) = (&params, &pk, &circuit);
                            scope.spawn(move || prove(params, pk, circuit, seed))
                        })
                        .collect();
                    handles.into_iter().map(|handle| handle.join().unwrap()).collect()
                });
                assert!(accel::counters().since(&before).msm_gpu > 0);
                assert!(proofs == expected, "a concurrent accelerated proof differs");
            }
        }
    };
}

prover_suite!(
    proofs_0_7,
    midnight_proofs_v7,
    midnight_curves_v2,
    rng_first
);
prover_suite!(
    proofs_0_8,
    midnight_proofs_v8,
    midnight_curves_v3,
    transcript_first
);

#[test]
fn install_is_idempotent_and_reports_a_device_when_active() {
    let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let first = accel::install();
    assert_eq!(accel::install(), first);
    assert_eq!(
        accel::device_name().is_some(),
        first != accel::Availability::NoDevice
    );
    assert_eq!(accel::is_active(), first == accel::Availability::Active);
}
