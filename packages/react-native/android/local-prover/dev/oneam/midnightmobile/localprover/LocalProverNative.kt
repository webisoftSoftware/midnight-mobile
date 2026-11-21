package dev.oneam.midnightmobile.localprover

import com.sun.jna.Library
import com.sun.jna.Pointer
import com.sun.jna.Structure

// The C interface of the native local prover (crates/runtime/src/local_prover/ffi.rs and
// accelerator.rs) and the structures it passes.

// JNA instantiates these reflectively from com.sun.jna.Klass, so they must be
// public with a public no-arg constructor. Kotlin's `private` compiles to
// package-private and makes every configure fail with IllegalAccessException.
@Structure.FieldOrder("k", "bytes", "bytesLen", "sha256")
open class ParameterDescriptor : Structure() {
  @JvmField var k: Int = 0
  @JvmField var bytes: Pointer? = null
  @JvmField var bytesLen: Long = 0
  @JvmField var sha256: Pointer? = null
}

@Structure.FieldOrder(
  "keyLocation",
  "keyLocationLen",
  "proverKey",
  "proverKeyLen",
  "proverKeySha256",
  "verifierKey",
  "verifierKeyLen",
  "verifierKeySha256",
  "ir",
  "irLen",
  "irSha256",
)
open class CircuitDescriptor : Structure() {
  @JvmField var keyLocation: Pointer? = null
  @JvmField var keyLocationLen: Long = 0
  @JvmField var proverKey: Pointer? = null
  @JvmField var proverKeyLen: Long = 0
  @JvmField var proverKeySha256: Pointer? = null
  @JvmField var verifierKey: Pointer? = null
  @JvmField var verifierKeyLen: Long = 0
  @JvmField var verifierKeySha256: Pointer? = null
  @JvmField var ir: Pointer? = null
  @JvmField var irLen: Long = 0
  @JvmField var irSha256: Pointer? = null
}

@Structure.FieldOrder("bytes", "bytesLen")
open class NativeResponse : Structure() {
  @JvmField var bytes: Pointer? = null
  @JvmField var bytesLen: Long = 0

  class ByReference : NativeResponse(), Structure.ByReference
}

// JNA instantiates this reflectively, so it stays public with a no-arg constructor; field order
// must exactly match Rust's #[repr(C)] LocalProverRequestDescriptor (bytes, then bytes_len).
@Structure.FieldOrder("bytes", "bytesLen")
open class RequestDescriptor : Structure() {
  @JvmField var bytes: Pointer? = null
  @JvmField var bytesLen: Long = 0
}

internal interface LocalProverNative : Library {
  fun midnight_mobile_local_prover_configure(
    parameters: Pointer?,
    parameterCount: Long,
    circuits: Pointer?,
    circuitCount: Long,
    outputHandle: LongByReference,
  ): Int

  fun midnight_mobile_local_prover_check(
    handle: Long,
    request: Pointer,
    requestLen: Long,
    output: NativeResponse.ByReference,
  ): Int

  fun midnight_mobile_local_prover_prove(
    handle: Long,
    request: Pointer,
    requestLen: Long,
    output: NativeResponse.ByReference,
  ): Int

  fun midnight_mobile_local_prover_prove_batch(
    handle: Long,
    requests: Pointer?,
    requestCount: Long,
    outputs: Pointer?,
  ): Int

  fun midnight_mobile_local_prover_cancel(handle: Long): Int

  fun midnight_mobile_local_prover_close(handle: Long): Int

  fun midnight_mobile_local_prover_set_max_concurrency(limit: Long): Int

  fun midnight_mobile_local_prover_set_profiling(enabled: Boolean): Int

  fun midnight_mobile_local_prover_take_timings(output: NativeResponse.ByReference): Int

  fun midnight_mobile_local_prover_set_accelerator(enabled: Boolean): Int

  fun midnight_mobile_local_prover_accelerator_status(output: NativeResponse.ByReference): Int

  fun midnight_mobile_local_prover_trim_memory(): Int

  fun midnight_mobile_local_prover_free(bytes: Pointer?, bytesLen: Long)
}
