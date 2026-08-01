import Darwin
import ExpoModulesCore
import Foundation
import MidnightMobileRuntime

private let localProverQueue = DispatchQueue(
  label: "dev.oneam.midnightmobile.local-prover",
  qos: .userInitiated,
  attributes: .concurrent
)

private struct LocalProverFileDefinition: Decodable {
  let uri: String
  let size: UInt64
  let sha256: String
}

private struct LocalProverParameterDefinition: Decodable {
  let k: UInt32
  let file: LocalProverFileDefinition
}

private struct LocalProverCircuitDefinition: Decodable {
  let keyLocation: String
  let proverKey: LocalProverFileDefinition
  let verifierKey: LocalProverFileDefinition
  let ir: LocalProverFileDefinition
}

private struct LocalProverConfiguration: Decodable {
  let parameters: [LocalProverParameterDefinition]
  let circuits: [LocalProverCircuitDefinition]
}

private struct LocalProverBridgeFailure: Error {
  let code: String
}

private final class StableBytes {
  private let storage: NSData

  init(_ bytes: [UInt8]) {
    storage = bytes.withUnsafeBytes { pointer in
      NSData(bytes: pointer.baseAddress, length: pointer.count)
    }
  }

  var pointer: UnsafePointer<UInt8> {
    storage.bytes.assumingMemoryBound(to: UInt8.self)
  }
}

private final class MappedLocalProverFile {
  let count: Int
  let hash: StableBytes
  private let address: UnsafeMutableRawPointer

  init(definition: LocalProverFileDefinition) throws {
    let decodedHash = try Self.decodeHash(definition.sha256)
    let file = try Self.resolve(definition.uri)
    let descriptor = Darwin.open(file.path, O_RDONLY | O_CLOEXEC)
    guard descriptor >= 0 else { throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION") }
    defer { Darwin.close(descriptor) }

    var metadata = stat()
    guard fstat(descriptor, &metadata) == 0,
          metadata.st_size > 0,
          UInt64(metadata.st_size) == definition.size,
          let mappedCount = Int(exactly: metadata.st_size) else {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    guard let mapped = mmap(nil, mappedCount, PROT_READ, MAP_PRIVATE, descriptor, 0),
          mapped != MAP_FAILED else {
      throw LocalProverBridgeFailure(code: "RESOURCE_PREFLIGHT_FAILED")
    }
    address = mapped
    count = mappedCount
    hash = StableBytes(decodedHash)
  }

  deinit {
    munmap(address, count)
  }

  var pointer: UnsafePointer<UInt8> {
    UnsafeRawPointer(address).assumingMemoryBound(to: UInt8.self)
  }

  private static func resolve(_ uri: String) throws -> URL {
    guard !uri.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    if uri.hasPrefix("bundle://") {
      let relative = String(uri.dropFirst("bundle://".count))
      guard !relative.isEmpty, !relative.hasPrefix("/") else {
        throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
      }
      guard let resourceRoot = Bundle.main.resourceURL else {
        throw LocalProverBridgeFailure(code: "RESOURCE_PREFLIGHT_FAILED")
      }
      return try containedFile(relative, root: resourceRoot)
    }
    guard uri.hasPrefix("/") else {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    let root = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
    return try containedFile(uri, root: root, absolute: true)
  }

  private static func containedFile(
    _ path: String,
    root: URL,
    absolute: Bool = false
  ) throws -> URL {
    let canonicalRoot = root.standardizedFileURL.resolvingSymlinksInPath()
    let candidate = (absolute ? URL(fileURLWithPath: path) : root.appendingPathComponent(path))
      .standardizedFileURL.resolvingSymlinksInPath()
    let prefix = canonicalRoot.path.hasSuffix("/")
      ? canonicalRoot.path
      : "\(canonicalRoot.path)/"
    guard candidate.path.hasPrefix(prefix) else {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    return candidate
  }

  private static func decodeHash(_ value: String) throws -> [UInt8] {
    let encoded = Array(value.utf8)
    guard encoded.count == 64 else {
      throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
    }
    return try stride(from: 0, to: encoded.count, by: 2).map { index in
      guard let high = nibble(encoded[index]), let low = nibble(encoded[index + 1]) else {
        throw LocalProverBridgeFailure(code: "INVALID_CONFIGURATION")
      }
      return high << 4 | low
    }
  }

  private static func nibble(_ value: UInt8) -> UInt8? {
    switch value {
    case 48...57: value - 48
    case 97...102: value - 87
    default: nil
    }
  }
}

private struct NativeLocalProverConfiguration {
  let parameters: [MidnightMobileLocalProverParameterDescriptor]
  let circuits: [MidnightMobileLocalProverCircuitDescriptor]
  let mappings: [MappedLocalProverFile]
  let locations: [StableBytes]
}

private struct LocalProverState {
  let handle: UInt64
  let mappings: [MappedLocalProverFile]
}

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
      "NATIVE_INTERNAL",
    ]
    let index = Int(code) - 1
    throw LocalProverBridgeFailure(
      code: codes.indices.contains(index) ? codes[index] : "NATIVE_INTERNAL"
    )
  }
}

private func localProverException(_ error: Error, fallback: String) -> Exception {
  let code = (error as? LocalProverBridgeFailure)?.code ?? fallback
  return Exception(name: "MidnightMobileLocalProverError", description: code, code: code)
}

public final class MidnightMobileLocalProverModule: Module {
  private let bridge = LocalProverBridge()

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

    AsyncFunction("close") {
      do {
        try bridge.close()
      } catch {
        throw localProverException(error, fallback: "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    OnDestroy {
      bridge.destroy()
    }
  }
}
