import Darwin
import ExpoModulesCore
import Foundation
import MidnightMobileRuntime
import UIKit

private let localProverQueue = DispatchQueue(
  label: "dev.oneam.midnightmobile.local-prover",
  qos: .userInitiated,
  attributes: .concurrent
)

private final class LocalProverBridge {
  private let lock = NSLock()
  private var state: LocalProverState?

  func configure(_ configurationJson: String) throws -> UInt64 {
    let definition: LocalProverConfiguration
    do {
      definition = try JSONDecoder().decode(
        LocalProverConfiguration.self,
        from: Data(configurationJson.utf8)
      )
    } catch {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    let configuration = try nativeConfiguration(definition)
    var outputHandle: UInt64 = 0
    let code = withExtendedLifetime(configuration) {
      configuration.parameters.withUnsafeBufferPointer { parameters in
        configuration.circuits.withUnsafeBufferPointer { circuits in
          midnight_mobile_local_prover_configure(
            parameters.baseAddress,
            parameters.count,
            circuits.baseAddress,
            circuits.count,
            &outputHandle
          )
        }
      }
    }
    try requireSuccess(code)
    synchronized {
      state = LocalProverState(handle: outputHandle, mappings: configuration.mappings)
    }
    return outputHandle
  }

  func check(_ request: Data) throws -> Data {
    try execute(request, prove: false)
  }

  func prove(_ request: Data) throws -> Data {
    try execute(request, prove: true)
  }

  func proveBatch(_ requests: [Data]) throws -> [Data] {
    guard let current = synchronized({ state }) else {
      throw LocalProverBridgeFailure(code: "STALE_REGISTRY")
    }
    guard !requests.isEmpty else {
      throw LocalProverBridgeFailure(code: "INVALID_REQUEST")
    }
    guard requests.count <= 64 else {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    for request in requests where request.isEmpty {
      throw LocalProverBridgeFailure(code: "INVALID_REQUEST")
    }

    let owned = requests.map(MutableRequestCopy.init)
    defer { owned.forEach { $0.wipe() } }

    let descriptors = owned.map { copy in
      MidnightMobileLocalProverRequestDescriptor(bytes: copy.pointer, bytes_len: copy.count)
    }
    var outputs = [MidnightMobileLocalProverResponse](
      repeating: MidnightMobileLocalProverResponse(bytes: nil, bytes_len: 0),
      count: owned.count
    )
    let code = withExtendedLifetime(owned) {
      descriptors.withUnsafeBufferPointer { descriptorBuffer in
        outputs.withUnsafeMutableBufferPointer { outputBuffer -> Int32 in
          midnight_mobile_local_prover_prove_batch(
            current.handle,
            descriptorBuffer.baseAddress,
            descriptorBuffer.count,
            outputBuffer.baseAddress
          )
        }
      }
    }
    defer {
      for output in outputs where output.bytes != nil {
        midnight_mobile_local_prover_free(output.bytes, output.bytes_len)
      }
    }
    try requireSuccess(code)
    return try outputs.map { response in
      guard let responseBytes = response.bytes, response.bytes_len > 0 else {
        throw LocalProverBridgeFailure(code: "NATIVE_INTERNAL")
      }
      return Data(bytes: responseBytes, count: response.bytes_len)
    }
  }

  func cancel() {
    guard let current = synchronized({ state }) else { return }
    _ = midnight_mobile_local_prover_cancel(current.handle)
  }

  /// Per-proof stage instrumentation. Process-wide and registry-independent, so it takes no
  /// handle and stays usable while a registry is being replaced.
  func setProfiling(_ enabled: Bool) throws {
    try requireSuccess(midnight_mobile_local_prover_set_profiling(enabled))
  }

  /// Drains recorded samples as the JSON the platform layer forwards verbatim. Empties the native
  /// queue, so a second call with no proofs in between yields `[]`.
  func takeTimings() throws -> String {
    var response = MidnightMobileLocalProverResponse(bytes: nil, bytes_len: 0)
    let code = midnight_mobile_local_prover_take_timings(&response)
    defer {
      if response.bytes != nil {
        midnight_mobile_local_prover_free(response.bytes, response.bytes_len)
      }
    }
    try requireSuccess(code)
    guard let responseBytes = response.bytes, response.bytes_len > 0 else { return "[]" }
    let data = Data(bytes: responseBytes, count: response.bytes_len)
    guard let json = String(data: data, encoding: .utf8) else {
      throw LocalProverBridgeFailure(code: "NATIVE_INTERNAL")
    }
    return json
  }

  func close() throws {
    guard let current = synchronized({ state }) else { return }
    let code = midnight_mobile_local_prover_close(current.handle)
    if code != 0 && code != 8 { try requireSuccess(code) }
    synchronized {
      if state?.handle == current.handle { state = nil }
    }
  }

  func destroy() {
    try? close()
  }

  private func execute(_ request: Data, prove: Bool) throws -> Data {
    guard let current = synchronized({ state }) else {
      throw LocalProverBridgeFailure(code: "STALE_REGISTRY")
    }
    guard !request.isEmpty else {
      throw LocalProverBridgeFailure(code: "INVALID_REQUEST")
    }
    var ownedRequest = request.withUnsafeBytes { Data($0) }
    defer { ownedRequest.resetBytes(in: 0..<ownedRequest.count) }
    var response = MidnightMobileLocalProverResponse(bytes: nil, bytes_len: 0)
    let code = ownedRequest.withUnsafeBytes { rawBytes -> Int32 in
      let bytes = rawBytes.baseAddress?.assumingMemoryBound(to: UInt8.self)
      guard let bytes else { return 5 }
      return prove
        ? midnight_mobile_local_prover_prove(
            current.handle, bytes, rawBytes.count, &response)
        : midnight_mobile_local_prover_check(
            current.handle, bytes, rawBytes.count, &response)
    }
    defer {
      if response.bytes != nil {
        midnight_mobile_local_prover_free(response.bytes, response.bytes_len)
      }
    }
    try requireSuccess(code)
    guard let responseBytes = response.bytes, response.bytes_len > 0 else {
      throw LocalProverBridgeFailure(code: "NATIVE_INTERNAL")
    }
    return Data(bytes: responseBytes, count: response.bytes_len)
  }

  private func nativeConfiguration(
    _ definition: LocalProverConfiguration
  ) throws -> NativeLocalProverConfiguration {
    try preflight(definition)
    var parameters = [MidnightMobileLocalProverParameterDescriptor]()
    var circuits = [MidnightMobileLocalProverCircuitDescriptor]()
    var mappings = [MappedLocalProverFile]()
    var locations = [StableBytes]()
    for parameter in definition.parameters {
      guard parameter.k <= 255 else {
        throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
      }
      let mapped = try MappedLocalProverFile(definition: parameter.file)
      parameters.append(MidnightMobileLocalProverParameterDescriptor(
        k: parameter.k,
        bytes: mapped.pointer,
        bytes_len: mapped.count,
        sha256: mapped.hash.pointer
      ))
      mappings.append(mapped)
    }
    for circuit in definition.circuits {
      let locationBytes = Array(circuit.keyLocation.utf8)
      guard !locationBytes.isEmpty, locationBytes.count <= 1_024,
            !circuit.keyLocation.unicodeScalars.contains(
              where: CharacterSet.controlCharacters.contains
            ) else {
        throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
      }
      let location = StableBytes(locationBytes)
      let prover = try MappedLocalProverFile(definition: circuit.proverKey)
      let verifier = try MappedLocalProverFile(definition: circuit.verifierKey)
      let ir = try MappedLocalProverFile(definition: circuit.ir)
      circuits.append(MidnightMobileLocalProverCircuitDescriptor(
        key_location: location.pointer,
        key_location_len: locationBytes.count,
        prover_key: prover.pointer,
        prover_key_len: prover.count,
        prover_key_sha256: prover.hash.pointer,
        verifier_key: verifier.pointer,
        verifier_key_len: verifier.count,
        verifier_key_sha256: verifier.hash.pointer,
        ir: ir.pointer,
        ir_len: ir.count,
        ir_sha256: ir.hash.pointer
      ))
      locations.append(location)
      mappings.append(contentsOf: [prover, verifier, ir])
    }
    return NativeLocalProverConfiguration(
      parameters: parameters,
      circuits: circuits,
      mappings: mappings,
      locations: locations
    )
  }

  private func preflight(_ definition: LocalProverConfiguration) throws {
    guard !definition.parameters.isEmpty,
          definition.parameters.count <= 32,
          definition.circuits.count <= 256 else {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    var parameterKs = Set<UInt32>()
    var locations = Set<String>()
    var totalBytes: UInt64 = 0
    let maximumFileBytes: UInt64 = 512 * 1_024 * 1_024
    let maximumTotalBytes: UInt64 = 2 * 1_024 * 1_024 * 1_024
    let files = definition.parameters.map(\.file) + definition.circuits.flatMap {
      [$0.proverKey, $0.verifierKey, $0.ir]
    }
    for parameter in definition.parameters {
      guard parameter.k <= 255, parameterKs.insert(parameter.k).inserted else {
        throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
      }
    }
    for circuit in definition.circuits {
      let locationBytes = Array(circuit.keyLocation.utf8)
      guard !locationBytes.isEmpty, locationBytes.count <= 1_024,
            !circuit.keyLocation.unicodeScalars.contains(
              where: CharacterSet.controlCharacters.contains
            ), locations.insert(circuit.keyLocation).inserted else {
        throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
      }
    }
    for file in files {
      let (nextTotal, overflow) = totalBytes.addingReportingOverflow(file.size)
      guard file.size > 0, file.size <= maximumFileBytes,
            !overflow, nextTotal <= maximumTotalBytes else {
        throw LocalProverBridgeFailure(code: "RESOURCE_PREFLIGHT_FAILED")
      }
      totalBytes = nextTotal
    }
  }

  private func synchronized<T>(_ operation: () throws -> T) rethrows -> T {
    lock.lock()
    defer { lock.unlock() }
    return try operation()
  }

  private func requireSuccess(_ code: Int32) throws {
    guard code != 0 else { return }
    let codes = [
      "INVALID_REQUEST", "UNSUPPORTED_CIRCUIT", "INTEGRITY_CHECK_FAILED",
      "PROVER_BUSY", "RESOURCE_PREFLIGHT_FAILED", "PROOF_FAILED",
      "INVALID_CONFIGURATION", "STALE_REGISTRY", "CHECK_FAILED",
      "NATIVE_INTERNAL", "CIRCUIT_TOO_LARGE",
    ]
    // The native status packs the error in its low byte; only CIRCUIT_TOO_LARGE uses
    // the bits above, where it carries the circuit size it needed.
    let index = Int(code & Self.statusCodeMask) - 1
    guard codes.indices.contains(index) else {
      throw LocalProverBridgeFailure(code: "NATIVE_INTERNAL")
    }
    if codes[index] == "CIRCUIT_TOO_LARGE" {
      let requiredK = (Int(code) >> Self.statusPayloadShift) & Int(Self.statusCodeMask)
      throw LocalProverBridgeFailure(code: "CIRCUIT_TOO_LARGE k=\(requiredK)")
    }
    throw LocalProverBridgeFailure(code: codes[index])
  }

  /// Low byte of a native status holds the error identity; see `ffi.rs`.
  private static let statusCodeMask: Int32 = 0xFF
  /// Bit offset of the `k` payload in a CIRCUIT_TOO_LARGE status.
  private static let statusPayloadShift = 8
}

private func localProverException(_ error: Error, fallback: String) -> Exception {
  let code = (error as? LocalProverBridgeFailure)?.code ?? fallback
  return Exception(name: "MidnightMobileLocalProverError", description: code, code: code)
}

public final class MidnightMobileLocalProverModule: Module {
  private let bridge = LocalProverBridge()
  private var memoryWarningObserver: NSObjectProtocol?

  public func definition() -> ModuleDefinition {
    Name("MidnightMobileLocalProver")

    AsyncFunction("configure") { (configurationJson: String) -> Double in
      do {
        return Double(try bridge.configure(configurationJson))
      } catch {
        throw localProverException(error, fallback: "INVALID_CONFIGURATION")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("check") { (request: Data) -> Data in
      do {
        return try bridge.check(request)
      } catch {
        throw localProverException(error, fallback: "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("prove") { (request: Data) -> Data in
      do {
        return try bridge.prove(request)
      } catch {
        throw localProverException(error, fallback: "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("proveBatch") { (requests: [Data]) -> [Data] in
      do {
        return try bridge.proveBatch(requests)
      } catch {
        throw localProverException(error, fallback: "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("cancel") {
      bridge.cancel()
    }.runOnQueue(localProverQueue)

    AsyncFunction("setProfiling") { (enabled: Bool) in
      do {
        try bridge.setProfiling(enabled)
      } catch {
        throw localProverException(error, fallback: "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("takeTimings") { () -> String in
      do {
        return try bridge.takeTimings()
      } catch {
        throw localProverException(error, fallback: "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("close") {
      do {
        try bridge.close()
      } catch {
        throw localProverException(error, fallback: "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    OnCreate {
      // Phase 5 memory pressure hook: drop the admission limit to 1 and refuse new admissions
      // above that until the process is relaunched. `set_max_concurrency` only affects permits
      // acquired after this call; already-admitted proofs run to completion.
      self.memoryWarningObserver = NotificationCenter.default.addObserver(
        forName: UIApplication.didReceiveMemoryWarningNotification,
        object: nil,
        queue: nil
      ) { _ in
        _ = midnight_mobile_local_prover_set_max_concurrency(1)
      }
    }

    OnDestroy {
      bridge.destroy()
      if let observer = self.memoryWarningObserver {
        NotificationCenter.default.removeObserver(observer)
        self.memoryWarningObserver = nil
      }
    }
  }
}
