import ExpoModulesCore
import Foundation

private func runtimeException(_ error: Error) -> Exception {
  let code: String
  switch error {
  case MidnightRuntimeError.Unavailable:
    code = "UNAVAILABLE"
  case MidnightRuntimeError.InvalidArgument:
    code = "INVALID_ARGUMENT"
  case MidnightRuntimeError.StaleSession:
    code = "STALE_SESSION"
  case MidnightRuntimeError.Cancelled:
    code = "CANCELLED"
  case MidnightRuntimeError.StateIncompatible:
    code = "STATE_INCOMPATIBLE"
  case MidnightRuntimeError.SyncGap:
    code = "SYNC_GAP"
  case MidnightRuntimeError.ProofFailed:
    code = "PROOF_FAILED"
  case MidnightRuntimeError.InsufficientDust:
    code = "INSUFFICIENT_DUST"
  case MidnightRuntimeError.SubmissionStatusUnknown:
    code = "SUBMISSION_STATUS_UNKNOWN"
  case MidnightRuntimeError.InvalidLength:
    code = "INVALID_LENGTH"
  case MidnightRuntimeError.DecodeFailed:
    code = "DECODE_FAILED"
  case MidnightRuntimeError.NativeInternal:
    code = "NATIVE_INTERNAL"
  default:
    code = "NATIVE_INTERNAL"
  }
  return Exception(name: "MidnightRuntimeError", description: code, code: code)
}

private func decodeBase64(_ value: String) throws -> Data {
  guard let data = Data(base64Encoded: value),
        data.base64EncodedString() == value else {
    throw MidnightRuntimeError.InvalidArgument
  }
  return data
}

private func decodeUInt64(_ value: Double) throws -> UInt64 {
  guard value.isFinite, value >= 0, value.rounded(.towardZero) == value,
        value <= 9_007_199_254_740_991 else {
    throw MidnightRuntimeError.InvalidArgument
  }
  return UInt64(value)
}

public class MidnightMobileRuntimeModule: Module {
  public func definition() -> ModuleDefinition {
    Name("MidnightMobileRuntime")

    AsyncFunction("openWalletSession") {
      (
        configJson: String,
        nightExternalKeyBytes: Data,
        zswapSeedBytes: Data,
        dustSeedBytes: Data,
        checkpointBase64: String?
      ) -> [String: Double] in
      var nightExternalKey = nightExternalKeyBytes
      var zswapSeed = zswapSeedBytes
      var dustSeed = dustSeedBytes
      var checkpoint = Data()
      do {
        defer {
          nightExternalKey.resetBytes(in: 0..<nightExternalKey.count)
          zswapSeed.resetBytes(in: 0..<zswapSeed.count)
          dustSeed.resetBytes(in: 0..<dustSeed.count)
          checkpoint.resetBytes(in: 0..<checkpoint.count)
        }
        checkpoint = try checkpointBase64.map(decodeBase64) ?? Data()
        let value = try openWalletSession(
          configJson: configJson,
          nightExternalKey: nightExternalKey,
          zswapSeed: zswapSeed,
          dustSeed: dustSeed,
          checkpoint: checkpointBase64 == nil ? nil : checkpoint
        )
        return ["id": Double(value.id), "generation": Double(value.generation)]
      } catch {
        throw runtimeException(error)
      }
    }

    AsyncFunction("applySyncBatch") {
      (
        sessionId: Double,
        generation: Double,
        stream: String,
        fromOffset: Double,
        toOffset: Double,
        payloadsBase64: [String]
      ) -> String in
      var payloads = [Data]()
      do {
        defer {
          for index in payloads.indices {
            payloads[index].resetBytes(in: 0..<payloads[index].count)
          }
        }
        payloads = try payloadsBase64.map(decodeBase64)
        return try applySyncBatch(
          sessionId: decodeUInt64(sessionId),
          generation: decodeUInt64(generation),
          stream: stream,
          fromOffset: decodeUInt64(fromOffset),
          toOffset: decodeUInt64(toOffset),
          payloads: payloads
        )
      } catch {
        throw runtimeException(error)
      }
    }

    AsyncFunction("getWalletSnapshot") {
      (sessionId: Double, generation: Double) -> String in
      do {
        return try getWalletSnapshot(
          sessionId: decodeUInt64(sessionId),
          generation: decodeUInt64(generation)
        )
      } catch {
        throw runtimeException(error)
      }
    }

    AsyncFunction("exportWalletCheckpoint") {
      (sessionId: Double, generation: Double) -> String in
      do {
        let checkpoint = try exportWalletCheckpoint(
          sessionId: decodeUInt64(sessionId),
          generation: decodeUInt64(generation)
        )
        return checkpoint.base64EncodedString()
      } catch {
        throw runtimeException(error)
      }
    }

    AsyncFunction("beginCommand") {
      (sessionId: Double, generation: Double, commandJson: String) -> String in
      do {
        return try beginCommand(
          sessionId: decodeUInt64(sessionId),
          generation: decodeUInt64(generation),
          commandJson: commandJson
        )
      } catch {
        throw runtimeException(error)
      }
    }

    AsyncFunction("resumeOperation") {
      (
        operationId: Double,
        generation: Double,
        networkResultJson: String?
      ) -> String in
      do {
        return try resumeOperation(
          operationId: decodeUInt64(operationId),
          generation: decodeUInt64(generation),
          networkResultJson: networkResultJson
        )
      } catch {
        throw runtimeException(error)
      }
    }

    AsyncFunction("cancelOperation") {
      (operationId: Double, generation: Double) in
      do {
        try cancelOperation(
          operationId: decodeUInt64(operationId),
          generation: decodeUInt64(generation)
        )
      } catch {
        throw runtimeException(error)
      }
    }

    AsyncFunction("closeWalletSession") {
      (sessionId: Double, generation: Double) in
      do {
        try closeWalletSession(
          sessionId: decodeUInt64(sessionId),
          generation: decodeUInt64(generation)
        )
      } catch {
        throw runtimeException(error)
      }
    }
  }
}
