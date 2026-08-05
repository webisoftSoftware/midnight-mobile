import Darwin
import Foundation
import MidnightMobileRuntime

// Artifact decoding, stable byte buffers, and mmap-backed files for the local prover
// bridge. Split out of MidnightMobileLocalProverModule.swift to stay within the
// repository's per-file line budget; these types are internal to the Expo module rather
// than file-private only because the bridge now lives in a sibling file.

struct LocalProverFileDefinition: Decodable {
  let uri: String
  let size: UInt64
  let sha256: String
}

struct LocalProverParameterDefinition: Decodable {
  let k: UInt32
  let file: LocalProverFileDefinition
}

struct LocalProverCircuitDefinition: Decodable {
  let keyLocation: String
  let proverKey: LocalProverFileDefinition
  let verifierKey: LocalProverFileDefinition
  let ir: LocalProverFileDefinition
}

struct LocalProverConfiguration: Decodable {
  let parameters: [LocalProverParameterDefinition]
  let circuits: [LocalProverCircuitDefinition]
}

struct LocalProverBridgeFailure: Error {
  let code: String
}

final class StableBytes {
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

/// A mutable, stably-addressed copy of one batch request. Unlike `StableBytes` (backed by
/// immutable `NSData`), this is backed by `NSMutableData` so `wipe()` can zero the secret-bearing
/// bytes in place once the native call has consumed them -- mirroring the single-request
/// `execute` path's `ownedRequest.resetBytes` discipline, but for N simultaneous live pointers
/// instead of one scoped `withUnsafeBytes` closure.
final class MutableRequestCopy {
  private let storage: NSMutableData

  init(_ data: Data) {
    let backing = NSMutableData(length: data.count) ?? NSMutableData()
    data.withUnsafeBytes { raw in
      guard let baseAddress = raw.baseAddress, raw.count > 0 else { return }
      backing.replaceBytes(in: NSRange(location: 0, length: raw.count), withBytes: baseAddress)
    }
    storage = backing
  }

  var count: Int { storage.length }

  var pointer: UnsafePointer<UInt8> {
    // `mutableBytes` is the mutable raw pointer `wipe()` needs; the FFI only
    // reads through it, so hand out the immutable view.
    UnsafePointer(storage.mutableBytes.assumingMemoryBound(to: UInt8.self))
  }

  func wipe() {
    guard storage.length > 0 else { return }
    memset(storage.mutableBytes, 0, storage.length)
  }
}

final class MappedLocalProverFile {
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

struct NativeLocalProverConfiguration {
  let parameters: [MidnightMobileLocalProverParameterDescriptor]
  let circuits: [MidnightMobileLocalProverCircuitDescriptor]
  let mappings: [MappedLocalProverFile]
  let locations: [StableBytes]
}

struct LocalProverState {
  let handle: UInt64
  let mappings: [MappedLocalProverFile]
}
