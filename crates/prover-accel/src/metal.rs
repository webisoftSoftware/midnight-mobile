//! The Metal accelerator: the hook functions and their installation.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::{Availability, Counters, G1JacobianLimbs, Scalar, tables};

static OUTCOME: OnceLock<Availability> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
/// Open [`crate::Request`]s. The lock also orders the device's begin and end
/// calls, so a request that opens while the last one closes is not wiped
/// away from under its first operation.
static OPEN_REQUESTS: Mutex<usize> = Mutex::new(0);

struct ClassCounters {
    gpu: AtomicU64,
    cpu: AtomicU64,
}

impl ClassCounters {
    const fn new() -> Self {
        Self {
            gpu: AtomicU64::new(0),
            cpu: AtomicU64::new(0),
        }
    }

    fn count(&self, served: bool) {
        let counter = if served { &self.gpu } else { &self.cpu };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

static MSM: ClassCounters = ClassCounters::new();
static FFT: ClassCounters = ClassCounters::new();
static IFFT: ClassCounters = ClassCounters::new();

fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// The commitment MSM hook of both prover builds.
fn commit_msm(scalars: &[Scalar], bases: &[G1JacobianLimbs]) -> Option<G1JacobianLimbs> {
    let result = if enabled() {
        // A panic in the table cache must not reach the prover; the Metal
        // entry points catch their own.
        catch_unwind(AssertUnwindSafe(|| commit_msm_on_device(scalars, bases)))
            .ok()
            .flatten()
    } else {
        None
    };
    MSM.count(result.is_some());
    result
}

fn commit_msm_on_device(scalars: &[Scalar], bases: &[G1JacobianLimbs]) -> Option<G1JacobianLimbs> {
    // Decide on the lengths before any table is looked up or built.
    midnight_metal::msm_admission(scalars.len(), bases.len(), midnight_metal::MAX_MSM_LOG).ok()?;
    let table = tables::serving(bases)?;
    midnight_metal::msm_g1(scalars, table.prefix(bases.len())?)
}

/// The warm-up hook of both prover builds: builds the table of one base vector
/// ahead of the first proof that commits with it.
fn warm(bases: &[G1JacobianLimbs]) {
    if !enabled() {
        return;
    }
    // Nothing is returned to the prover, so a failure only costs the warm-up.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let len = bases.len();
        if midnight_metal::msm_admission(len, len, midnight_metal::MAX_MSM_LOG).is_ok() {
            let _ = tables::serving(bases);
        }
    }));
}

/// The FFT hook of both prover builds.
fn fft(values: &mut [Scalar], omega: Scalar, log_n: u32) -> bool {
    let served = enabled() && midnight_metal::fft(values, omega, log_n);
    FFT.count(served);
    served
}

/// The inverse FFT hook of both prover builds.
fn ifft(values: &mut [Scalar], omega_inv: Scalar, log_n: u32, divisor: Scalar) -> bool {
    let served = enabled() && midnight_metal::ifft(values, omega_inv, log_n, divisor);
    IFFT.count(served);
    served
}

/// The first iOS and macOS versions whose Metal runs the shaders, which are
/// written in Metal Shading Language 3.2.
const MINIMUM_IOS: isize = 18;
const MINIMUM_MACOS: isize = 15;

/// Whether this system's Metal runs the shaders. Asked before Metal is touched
/// at all, so an older system never loads the shader library.
fn system_runs_the_shaders() -> bool {
    let version = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
    let minimum = if cfg!(target_os = "ios") {
        MINIMUM_IOS
    } else {
        MINIMUM_MACOS
    };
    version.majorVersion >= minimum
}

pub(crate) fn install() -> Availability {
    *OUTCOME.get_or_init(|| {
        if !system_runs_the_shaders() {
            return Availability::NoDevice;
        }
        // `device` makes the context: the queue and the shader library.
        if midnight_metal::present().is_none() || !midnight_metal::device() {
            return Availability::NoDevice;
        }
        let v7 = midnight_proofs_v7::gpu::install(midnight_proofs_v7::gpu::GpuHooks {
            commit_msm: Some(commit_msm),
            fft: Some(fft),
            ifft: Some(ifft),
            warm: Some(warm),
        });
        let v8 = midnight_proofs_v8::gpu::install(midnight_proofs_v8::gpu::GpuHooks {
            commit_msm: Some(commit_msm),
            fft: Some(fft),
            ifft: Some(ifft),
            warm: Some(warm),
        });
        if v7 && v8 {
            Availability::Active
        } else {
            Availability::HooksTaken
        }
    })
}

pub(crate) fn set_enabled(value: bool) {
    ENABLED.store(value, Ordering::Release);
}

pub(crate) fn is_active() -> bool {
    enabled() && OUTCOME.get() == Some(&Availability::Active)
}

pub(crate) fn device_name() -> Option<String> {
    midnight_metal::present().map(|info| info.name.clone())
}

pub(crate) fn counters() -> Counters {
    let device = midnight_metal::stats();
    let (tables, table_points) = tables::usage();
    Counters {
        msm_gpu: MSM.gpu.load(Ordering::Relaxed),
        msm_cpu: MSM.cpu.load(Ordering::Relaxed),
        fft_gpu: FFT.gpu.load(Ordering::Relaxed),
        fft_cpu: FFT.cpu.load(Ordering::Relaxed),
        ifft_gpu: IFFT.gpu.load(Ordering::Relaxed),
        ifft_cpu: IFFT.cpu.load(Ordering::Relaxed),
        device_nanos: device
            .msm
            .device_nanos
            .saturating_add(device.fft.device_nanos)
            .saturating_add(device.ifft.device_nanos),
        tables,
        table_points,
    }
}

pub(crate) fn release_tables() {
    tables::clear();
}

pub(crate) fn trim_memory() -> u64 {
    tables::clear();
    // Only idle buffers are released: a buffer a running proof holds stays.
    midnight_metal::release_pool()
}

pub(crate) fn override_min_log_size(min_log_size: Option<u32>) {
    match min_log_size {
        Some(k) => {
            midnight_metal::set_min_k(k);
        }
        None => midnight_metal::reset_min_k(),
    }
}

pub(crate) fn begin_request() {
    let mut open = OPEN_REQUESTS.lock().unwrap_or_else(PoisonError::into_inner);
    if *open == 0 {
        midnight_metal::begin_request();
    }
    *open += 1;
}

pub(crate) fn end_request() {
    let mut open = OPEN_REQUESTS.lock().unwrap_or_else(PoisonError::into_inner);
    *open = open.saturating_sub(1);
    if *open == 0 {
        // A failed wipe is already handled inside the backend: the region is
        // zeroed again and released instead of pooled.
        let _ = midnight_metal::end_request();
    }
}
