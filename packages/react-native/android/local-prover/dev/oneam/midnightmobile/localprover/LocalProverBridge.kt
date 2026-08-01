package dev.oneam.midnightmobile.localprover

import android.content.res.AssetManager
import com.sun.jna.Library
import com.sun.jna.Memory
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.Structure
import com.sun.jna.ptr.LongByReference
import java.io.File
import java.io.FileInputStream
import java.nio.ByteBuffer
import java.nio.channels.FileChannel
import org.json.JSONObject

private const val HASH_BYTES = 32
private const val ASSET_PREFIX = "asset://"
private const val MAX_PARAMETERS = 32
private const val MAX_CIRCUITS = 256
private const val MAX_ARTIFACT_BYTES = 512L * 1024 * 1024
private const val MAX_TOTAL_ARTIFACT_BYTES = 2L * 1024 * 1024 * 1024
private const val MAX_KEY_LOCATION_BYTES = 1_024

data class LocalProverFile(val uri: String, val size: Long, val sha256: String)

data class LocalProverParameter(val k: Int, val file: LocalProverFile)

data class LocalProverCircuit(
  val keyLocation: String,
  val proverKey: LocalProverFile,
  val verifierKey: LocalProverFile,
  val ir: LocalProverFile,
)

data class LocalProverConfiguration(
  val parameters: List<LocalProverParameter>,
  val circuits: List<LocalProverCircuit>,
)

class LocalProverBridgeException(val stableCode: String) : IllegalStateException(stableCode)

@Structure.FieldOrder("k", "bytes", "bytesLen", "sha256")
private open class ParameterDescriptor : Structure() {
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
private open class CircuitDescriptor : Structure() {
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
private open class NativeResponse : Structure() {
  @JvmField var bytes: Pointer? = null
  @JvmField var bytesLen: Long = 0

  class ByReference : NativeResponse(), Structure.ByReference
}

private interface LocalProverNative : Library {
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

  fun midnight_mobile_local_prover_close(handle: Long): Int

  fun midnight_mobile_local_prover_free(bytes: Pointer?, bytesLen: Long)
}

private data class MappedFile(
  val definition: LocalProverFile,
  val bytes: ByteBuffer,
  val hash: ByteBuffer,
)

private data class RegistryState(val handle: Long, val mappings: List<MappedFile>)

private data class NativeConfiguration(
  val parameters: Array<ParameterDescriptor>,
  val circuits: Array<CircuitDescriptor>,
  val mappings: List<MappedFile>,
  val retainedBuffers: List<ByteBuffer>,
)

class LocalProverBridge(private val assets: AssetManager? = null) {
  private val lock = Any()
  private val native: LocalProverNative by lazy {
    Native.load("midnight_mobile_runtime", LocalProverNative::class.java)
  }
  private var state: RegistryState? = null

  fun configure(configuration: LocalProverConfiguration): Long {
    val bindings = nativeConfiguration(configuration)
    val handle = LongByReference(0)
    val code = native.midnight_mobile_local_prover_configure(
      bindings.parameters.firstOrNull()?.pointer,
      bindings.parameters.size.toLong(),
      bindings.circuits.firstOrNull()?.pointer,
      bindings.circuits.size.toLong(),
      handle,
    )
    keepAlive(bindings)
    requireSuccess(code)
    synchronized(lock) {
      state = RegistryState(handle.value, bindings.mappings)
    }
    return handle.value
  }

  fun check(request: ByteArray): ByteArray =
    execute(request, native::midnight_mobile_local_prover_check)

  fun prove(request: ByteArray): ByteArray =
    execute(request, native::midnight_mobile_local_prover_prove)

  fun close() {
    val current = synchronized(lock) { state } ?: return
    val code = native.midnight_mobile_local_prover_close(current.handle)
    if (code != 0 && code != 8) requireSuccess(code)
    synchronized(lock) {
      if (state?.handle == current.handle) state = null
    }
  }

  private fun execute(
    request: ByteArray,
    operation: (Long, Pointer, Long, NativeResponse.ByReference) -> Int,
  ): ByteArray {
    if (request.isEmpty()) throw LocalProverBridgeException("INVALID_REQUEST")
    val current = synchronized(lock) { state }
      ?: throw LocalProverBridgeException("STALE_REGISTRY")
    val requestMemory = Memory(request.size.toLong())
    requestMemory.write(0, request, 0, request.size)
    val output = NativeResponse.ByReference()
    return try {
      output.write()
      val code = operation(current.handle, requestMemory, request.size.toLong(), output)
      output.read()
      requireSuccess(code)
      val pointer = output.bytes ?: throw LocalProverBridgeException("NATIVE_INTERNAL")
      val size = Math.toIntExact(output.bytesLen)
      try {
        pointer.getByteArray(0, size)
      } finally {
        native.midnight_mobile_local_prover_free(pointer, output.bytesLen)
      }
    } finally {
      requestMemory.clear()
    }
  }

  private fun nativeConfiguration(configuration: LocalProverConfiguration): NativeConfiguration {
    validateConfiguration(configuration)
    val mappings = mutableListOf<MappedFile>()
    val retained = mutableListOf<ByteBuffer>()
    val parameterArray = descriptors<ParameterDescriptor>(configuration.parameters.size)
    configuration.parameters.forEachIndexed { index, parameter ->
      require(parameter.k in 0..255) { "parameter k must fit u8" }
      val mapped = map(parameter.file)
      mappings += mapped
      parameterArray[index].apply {
        k = parameter.k
        bytes = Native.getDirectBufferPointer(mapped.bytes)
        bytesLen = mapped.bytes.capacity().toLong()
        sha256 = Native.getDirectBufferPointer(mapped.hash)
        write()
      }
    }
    val circuitArray = descriptors<CircuitDescriptor>(configuration.circuits.size)
    configuration.circuits.forEachIndexed { index, circuit ->
      val location = direct(circuit.keyLocation.encodeToByteArray())
      val prover = map(circuit.proverKey)
      val verifier = map(circuit.verifierKey)
      val ir = map(circuit.ir)
      mappings += listOf(prover, verifier, ir)
      retained += location
      circuitArray[index].apply {
        keyLocation = Native.getDirectBufferPointer(location)
        keyLocationLen = location.capacity().toLong()
        proverKey = Native.getDirectBufferPointer(prover.bytes)
        proverKeyLen = prover.bytes.capacity().toLong()
        proverKeySha256 = Native.getDirectBufferPointer(prover.hash)
        verifierKey = Native.getDirectBufferPointer(verifier.bytes)
        verifierKeyLen = verifier.bytes.capacity().toLong()
        verifierKeySha256 = Native.getDirectBufferPointer(verifier.hash)
        this.ir = Native.getDirectBufferPointer(ir.bytes)
        irLen = ir.bytes.capacity().toLong()
        irSha256 = Native.getDirectBufferPointer(ir.hash)
        write()
      }
    }
    return NativeConfiguration(parameterArray, circuitArray, mappings, retained)
  }

  private fun validateConfiguration(configuration: LocalProverConfiguration) {
    require(configuration.parameters.size in 1..MAX_PARAMETERS) {
      "parameter count is outside the supported range"
    }
    require(configuration.circuits.size <= MAX_CIRCUITS) {
      "circuit count is outside the supported range"
    }
    require(configuration.parameters.map { it.k }.distinct().size == configuration.parameters.size) {
      "parameter k values must be unique"
    }
    require(
      configuration.circuits.map { it.keyLocation }.distinct().size ==
        configuration.circuits.size,
    ) {
      "circuit key locations must be unique"
    }
    configuration.circuits.forEach { circuit ->
      val location = circuit.keyLocation.encodeToByteArray()
      require(location.isNotEmpty() && location.size <= MAX_KEY_LOCATION_BYTES) {
        "circuit key location is outside the supported range"
      }
      require(circuit.keyLocation.none { it.code < 32 || it.code == 127 }) {
        "circuit key location contains a control character"
      }
    }
    val files = configuration.parameters.map { it.file } + configuration.circuits.flatMap {
      listOf(it.proverKey, it.verifierKey, it.ir)
    }
    var total = 0L
    files.forEach { file ->
      require(file.size in 1..MAX_ARTIFACT_BYTES) { "artifact size is outside the supported range" }
      total = Math.addExact(total, file.size)
      require(total <= MAX_TOTAL_ARTIFACT_BYTES) { "total artifact size is outside the supported range" }
    }
  }

  private inline fun <reified T : Structure> descriptors(count: Int): Array<T> {
    if (count == 0) return emptyArray()
    val first = T::class.java.getDeclaredConstructor().newInstance()
    @Suppress("UNCHECKED_CAST")
    return first.toArray(count) as Array<T>
  }

  private fun map(definition: LocalProverFile): MappedFile {
    require(definition.size > 0) { "artifact size must be positive" }
    val mapped = if (definition.uri.startsWith(ASSET_PREFIX)) {
      val manager = assets ?: throw IllegalArgumentException("asset:// requires Android assets")
      manager.openFd(definition.uri.removePrefix(ASSET_PREFIX)).use { descriptor ->
        require(descriptor.declaredLength == definition.size) { "artifact size mismatch" }
        FileInputStream(descriptor.fileDescriptor).channel.use { channel ->
          channel.map(FileChannel.MapMode.READ_ONLY, descriptor.startOffset, definition.size)
        }
      }
    } else {
      require(!definition.uri.startsWith("bundle://")) { "bundle:// is not supported on Android" }
      val file = File(definition.uri)
      require(file.isAbsolute && file.length() == definition.size) { "artifact size mismatch" }
      FileInputStream(file).channel.use { channel ->
        channel.map(FileChannel.MapMode.READ_ONLY, 0, definition.size)
      }
    }.asReadOnlyBuffer()
    require(mapped.isDirect) { "artifact mapping must be direct" }
    return MappedFile(definition, mapped, direct(parseHash(definition.sha256)))
  }

  private fun direct(bytes: ByteArray): ByteBuffer =
    ByteBuffer.allocateDirect(bytes.size).apply {
      put(bytes)
      flip()
    }.asReadOnlyBuffer()

  private fun parseHash(value: String): ByteArray {
    require(value.matches(Regex("^[0-9a-f]{64}$"))) { "SHA-256 must be lowercase hex" }
    return ByteArray(HASH_BYTES) { index ->
      value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
    }
  }

  private fun requireSuccess(code: Int) {
    if (code == 0) return
    val stable = listOf(
      "INVALID_REQUEST",
      "UNSUPPORTED_CIRCUIT",
      "INTEGRITY_CHECK_FAILED",
      "PROVER_BUSY",
      "RESOURCE_PREFLIGHT_FAILED",
      "PROOF_FAILED",
      "INVALID_CONFIGURATION",
      "STALE_REGISTRY",
      "CHECK_FAILED",
      "NATIVE_INTERNAL",
    ).getOrNull(code - 1) ?: "NATIVE_INTERNAL"
    throw LocalProverBridgeException(stable)
  }

  private fun keepAlive(configuration: NativeConfiguration) {
    configuration.parameters.forEach { it.autoWrite() }
    configuration.circuits.forEach { it.autoWrite() }
    configuration.mappings.forEach { mapped ->
      mapped.bytes.isReadOnly
      mapped.hash.isReadOnly
    }
    configuration.retainedBuffers.forEach { it.isDirect }
  }

  companion object {
    fun decodeConfiguration(json: String): LocalProverConfiguration {
      val root = JSONObject(json)
      val parameters = root.getJSONArray("parameters").let { values ->
        List(values.length()) { index ->
          val value = values.getJSONObject(index)
          LocalProverParameter(value.getInt("k"), decodeFile(value.getJSONObject("file")))
        }
      }
      val circuits = root.getJSONArray("circuits").let { values ->
        List(values.length()) { index ->
          val value = values.getJSONObject(index)
          LocalProverCircuit(
            value.getString("keyLocation"),
            decodeFile(value.getJSONObject("proverKey")),
            decodeFile(value.getJSONObject("verifierKey")),
            decodeFile(value.getJSONObject("ir")),
          )
        }
      }
      return LocalProverConfiguration(parameters, circuits)
    }

    private fun decodeFile(value: JSONObject): LocalProverFile = LocalProverFile(
      value.getString("uri"),
      value.getLong("size"),
      value.getString("sha256"),
    )
  }
}
