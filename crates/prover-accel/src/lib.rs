//! GPU acceleration for the local prover.
//!
//! The local prover links two builds of `midnight-proofs` (0.7.3 for Ledger 8
//! circuits, 0.8.3 for ZKIR v3 circuits). Both are the hooked copies under
//! `third_party/`, which offer three operations to an installed accelerator
//! before they run their own code: the KZG commitment MSM, the FFT and the
//! inverse FFT. This crate installs one accelerator into both builds.
//!
//! On Apple targets the accelerator is the Metal backend (`midnight-metal`).
//! On every other target [`install`] reports [`Availability::Unsupported`]
//! and the prover runs exactly the stock CPU code.
//!
//! # Contract
//!
//! - An accelerated operation returns the exact value of the stock CPU code
//!   or declines, and the stock code then runs on the unchanged input. A
//!   proof made with the accelerator is therefore the proof the CPU prover
//!   makes from the same randomness.
//! - Nothing here draws randomness or touches a transcript.
//! - A failure of the device, of a memory allocation or of this crate is a
//!   decline, never a panic into the prover.
//! - Device buffers that held witness values are zeroed when the last open
//!   [`Request`] ends.

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod metal;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod tables;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use metal as backend;

/// A scalar as `blst_fr` holds it: four little-endian `u64` limbs in
/// Montgomery form.
pub type Scalar = [u64; 4];
/// An affine G1 point as `blst_p1_affine` holds it: `x`, `y`, six limbs each.
pub type G1AffineLimbs = [u64; 12];
/// A Jacobian G1 point as `blst_p1` holds it: `X`, `Y`, `Z`, six limbs each.
pub type G1JacobianLimbs = [u64; 18];

/// Whether the accelerator serves this process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// The hooks are installed and the device is usable.
    Active,
    /// This build has no accelerator for its target.
    Unsupported,
    /// No usable device: none present, an operating system older than the
    /// shaders need, or the backend switched off by its environment variable.
    NoDevice,
    /// Another component installed prover hooks first.
    HooksTaken,
}

impl Availability {
    /// A stable name for logs and for the platform layer.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Unsupported => "unsupported",
            Self::NoDevice => "no-device",
            Self::HooksTaken => "hooks-taken",
        }
    }
}

/// How many prover operations ran on the device and how many on the CPU
/// since the process started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Commitment MSMs the device computed.
    pub msm_gpu: u64,
    /// Commitment MSMs offered to the accelerator that ran on the CPU.
    pub msm_cpu: u64,
    /// Forward FFTs the device computed.
    pub fft_gpu: u64,
    /// Forward FFTs offered to the accelerator that ran on the CPU.
    pub fft_cpu: u64,
    /// Inverse FFTs the device computed.
    pub ifft_gpu: u64,
    /// Inverse FFTs offered to the accelerator that ran on the CPU.
    pub ifft_cpu: u64,
    /// Device time of the served operations in nanoseconds.
    pub device_nanos: u64,
    /// SRS base tables currently cached for the device.
    pub tables: u64,
    /// Points in those tables.
    pub table_points: u64,
}

impl Counters {
    /// Operations the device computed.
    #[must_use]
    pub const fn gpu_operations(&self) -> u64 {
        self.msm_gpu + self.fft_gpu + self.ifft_gpu
    }

    /// The counters of `self` minus those of an earlier snapshot. The table
    /// fields are taken from `self`.
    #[must_use]
    pub const fn since(&self, earlier: &Self) -> Self {
        Self {
            msm_gpu: self.msm_gpu.saturating_sub(earlier.msm_gpu),
            msm_cpu: self.msm_cpu.saturating_sub(earlier.msm_cpu),
            fft_gpu: self.fft_gpu.saturating_sub(earlier.fft_gpu),
            fft_cpu: self.fft_cpu.saturating_sub(earlier.fft_cpu),
            ifft_gpu: self.ifft_gpu.saturating_sub(earlier.ifft_gpu),
            ifft_cpu: self.ifft_cpu.saturating_sub(earlier.ifft_cpu),
            device_nanos: self.device_nanos.saturating_sub(earlier.device_nanos),
            tables: self.tables,
            table_points: self.table_points,
        }
    }
}

/// Installs the accelerator into both prover builds. Idempotent: every call
/// returns the outcome of the first one. Never fails: without a usable device
/// the prover keeps running the stock CPU code.
pub fn install() -> Availability {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        backend::install()
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        Availability::Unsupported
    }
}

/// Switches the accelerator on or off at run time. While it is off every
/// offered operation declines and the stock CPU code runs. On by default.
pub fn set_enabled(enabled: bool) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    backend::set_enabled(enabled);
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let _ = enabled;
}

/// Whether offered operations go to the device: [`install`] returned
/// [`Availability::Active`] and the accelerator is not switched off.
#[must_use]
pub fn is_active() -> bool {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        backend::is_active()
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        false
    }
}

/// The name of the device, when [`install`] found one.
#[must_use]
pub fn device_name() -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        backend::device_name()
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        None
    }
}

/// The operation counters since the process started.
#[must_use]
pub fn counters() -> Counters {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        backend::counters()
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        Counters::default()
    }
}

/// Frees the cached SRS base tables. The next accelerated commitment builds
/// its table again. Call it when the prover's parameters are released.
pub fn release_tables() {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    backend::release_tables();
}

/// Gives memory back under memory pressure: frees the cached SRS base tables
/// and the idle device buffers kept for reuse. Returns the bytes of idle
/// buffers released. Safe at any time, also while proofs run: a running proof
/// keeps what it holds, and the next accelerated operation allocates again.
pub fn trim_memory() -> u64 {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        backend::trim_memory()
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        0
    }
}

/// Replaces the size thresholds of the device with one explicit value
/// (`Some(k)`: operations of at least `2^k` elements go to the device), or
/// restores the measured defaults (`None`). For tuning and tests.
pub fn override_min_log_size(min_log_size: Option<u32>) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    backend::override_min_log_size(min_log_size);
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let _ = min_log_size;
}

/// One proving request. While at least one request is open, device buffers
/// are reused between operations; when the last one is dropped they are
/// zeroed and returned to the pool.
#[derive(Debug)]
#[must_use = "the request ends when this value is dropped"]
pub struct Request {
    _private: (),
}

impl Request {
    /// Opens a request.
    pub fn begin() -> Self {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        backend::begin_request();
        Self { _private: () }
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        backend::end_request();
    }
}
