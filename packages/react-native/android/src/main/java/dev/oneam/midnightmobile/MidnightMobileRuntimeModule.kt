package dev.oneam.midnightmobile

import android.util.Base64
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import dev.oneam.midnightmobile.uniffi.MidnightRuntimeException
import dev.oneam.midnightmobile.uniffi.applySyncBatch as runtimeApplySyncBatch
import dev.oneam.midnightmobile.uniffi.beginCommand as runtimeBeginCommand
import dev.oneam.midnightmobile.uniffi.cancelOperation as runtimeCancelOperation
import dev.oneam.midnightmobile.uniffi.closeWalletSession as runtimeCloseWalletSession
import dev.oneam.midnightmobile.uniffi.exportWalletCheckpoint as runtimeExportWalletCheckpoint
import dev.oneam.midnightmobile.uniffi.getWalletSnapshot as runtimeGetWalletSnapshot
import dev.oneam.midnightmobile.uniffi.openWalletSession as runtimeOpenWalletSession
import dev.oneam.midnightmobile.uniffi.resumeOperation as runtimeResumeOperation

private fun decodeBase64(value: String): ByteArray {
  val decoded = try {
    Base64.decode(value, Base64.NO_WRAP)
  } catch (error: IllegalArgumentException) {
    throw MidnightRuntimeException.InvalidArgument()
  }
  if (Base64.encodeToString(decoded, Base64.NO_WRAP) != value) {
    decoded.fill(0)
    throw MidnightRuntimeException.InvalidArgument()
  }
  return decoded
}

private fun decodeULong(value: Double): ULong {
  if (!value.isFinite() || value < 0 || value % 1.0 != 0.0 ||
    value > 9_007_199_254_740_991.0
  ) {
    throw MidnightRuntimeException.InvalidArgument()
  }
  return value.toLong().toULong()
}

private fun runtimeException(error: Throwable): CodedException {
  val code = when (error) {
    is MidnightRuntimeException.Unavailable -> "UNAVAILABLE"
    is MidnightRuntimeException.InvalidArgument -> "INVALID_ARGUMENT"
    is MidnightRuntimeException.StaleSession -> "STALE_SESSION"
    is MidnightRuntimeException.Cancelled -> "CANCELLED"
    is MidnightRuntimeException.StateIncompatible -> "STATE_INCOMPATIBLE"
    is MidnightRuntimeException.SyncGap -> "SYNC_GAP"
    is MidnightRuntimeException.ProofFailed -> "PROOF_FAILED"
    is MidnightRuntimeException.InsufficientDust -> "INSUFFICIENT_DUST"
    is MidnightRuntimeException.SubmissionStatusUnknown -> "SUBMISSION_STATUS_UNKNOWN"
    is MidnightRuntimeException.InvalidLength -> "INVALID_LENGTH"
    is MidnightRuntimeException.DecodeFailed -> "DECODE_FAILED"
    is MidnightRuntimeException.NativeInternal -> "NATIVE_INTERNAL"
    else -> "NATIVE_INTERNAL"
  }
  return CodedException(code, code, error)
}

class MidnightMobileRuntimeModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("MidnightMobileRuntime")

    AsyncFunction("openWalletSession") {
        configJson: String,
        nightExternalKey: ByteArray,
        zswapSeed: ByteArray,
        dustSeed: ByteArray,
        checkpointBase64: String? ->
      var checkpoint: ByteArray? = null
      try {
        checkpoint = checkpointBase64?.let(::decodeBase64)
        val value = runtimeOpenWalletSession(
          configJson,
          nightExternalKey,
          zswapSeed,
          dustSeed,
          checkpoint
        )
        mapOf("id" to value.id.toDouble(), "generation" to value.generation.toDouble())
      } catch (error: Throwable) {
        throw runtimeException(error)
      } finally {
        nightExternalKey.fill(0)
        zswapSeed.fill(0)
        dustSeed.fill(0)
        checkpoint?.fill(0)
      }
    }

    AsyncFunction("applySyncBatch") {
        sessionId: Double,
        generation: Double,
        stream: String,
        fromOffset: Double,
        toOffset: Double,
        payloadsBase64: List<String> ->
      var payloads = emptyList<ByteArray>()
      try {
        payloads = payloadsBase64.map(::decodeBase64)
        runtimeApplySyncBatch(
          decodeULong(sessionId),
          decodeULong(generation),
          stream,
          decodeULong(fromOffset),
          decodeULong(toOffset),
          payloads
        )
      } catch (error: Throwable) {
        throw runtimeException(error)
      } finally {
        payloads.forEach { it.fill(0) }
      }
    }

    AsyncFunction("getWalletSnapshot") {
        sessionId: Double, generation: Double ->
      try {
        runtimeGetWalletSnapshot(decodeULong(sessionId), decodeULong(generation))
      } catch (error: Throwable) {
        throw runtimeException(error)
      }
    }

    AsyncFunction("exportWalletCheckpoint") {
        sessionId: Double, generation: Double ->
      try {
        val bytes = runtimeExportWalletCheckpoint(
          decodeULong(sessionId),
          decodeULong(generation)
        )
        try {
          Base64.encodeToString(bytes, Base64.NO_WRAP)
        } finally {
          bytes.fill(0)
        }
      } catch (error: Throwable) {
        throw runtimeException(error)
      }
    }

    AsyncFunction("beginCommand") {
        sessionId: Double, generation: Double, commandJson: String ->
      try {
        runtimeBeginCommand(decodeULong(sessionId), decodeULong(generation), commandJson)
      } catch (error: Throwable) {
        throw runtimeException(error)
      }
    }

    AsyncFunction("resumeOperation") {
        operationId: Double, generation: Double, networkResultJson: String? ->
      try {
        runtimeResumeOperation(
          decodeULong(operationId),
          decodeULong(generation),
          networkResultJson
        )
      } catch (error: Throwable) {
        throw runtimeException(error)
      }
    }

    AsyncFunction("cancelOperation") {
        operationId: Double, generation: Double ->
      try {
        runtimeCancelOperation(decodeULong(operationId), decodeULong(generation))
      } catch (error: Throwable) {
        throw runtimeException(error)
      }
    }

    AsyncFunction("closeWalletSession") {
        sessionId: Double, generation: Double ->
      try {
        runtimeCloseWalletSession(decodeULong(sessionId), decodeULong(generation))
      } catch (error: Throwable) {
        throw runtimeException(error)
      }
    }
  }
}
