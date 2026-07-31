package network.midnight.proverspike

import android.app.Activity
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.Gravity
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.Structure
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.nio.channels.FileChannel
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

private const val LOG_TAG = "EmbeddedProverSpike"

@Structure.FieldOrder(
  "request",
  "requestLen",
  "paramsK15",
  "paramsK15Len",
  "proverKey",
  "proverKeyLen",
  "verifierKey",
  "verifierKeyLen",
  "ir",
  "irLen",
)
private open class ProbeInputs : Structure() {
  @JvmField var request: Pointer? = null
  @JvmField var requestLen: Long = 0
  @JvmField var paramsK15: Pointer? = null
  @JvmField var paramsK15Len: Long = 0
  @JvmField var proverKey: Pointer? = null
  @JvmField var proverKeyLen: Long = 0
  @JvmField var verifierKey: Pointer? = null
  @JvmField var verifierKeyLen: Long = 0
  @JvmField var ir: Pointer? = null
  @JvmField var irLen: Long = 0

  class ByReference : ProbeInputs(), Structure.ByReference
}

@Structure.FieldOrder(
  "taggedProofBytes",
  "taggedProofBytesLen",
  "artifactDecodingMillis",
  "provingAndSelfVerificationMillis",
)
private open class ProbeResult : Structure() {
  @JvmField var taggedProofBytes: Pointer? = null
  @JvmField var taggedProofBytesLen: Long = 0
  @JvmField var artifactDecodingMillis: Long = 0
  @JvmField var provingAndSelfVerificationMillis: Long = 0

  class ByReference : ProbeResult(), Structure.ByReference
}

private interface ProverNative : Library {
  fun midnight_embedded_prover_probe(
    inputs: ProbeInputs.ByReference,
    output: ProbeResult.ByReference,
  ): Int

  fun midnight_embedded_prover_free(pointer: Pointer?, length: Long)
}

private data class MappedArtifact(val name: String, val bytes: ByteBuffer)

class MainActivity : Activity() {
  private val running = AtomicBoolean(false)
  private val executor = Executors.newSingleThreadExecutor()
  private val mainHandler = Handler(Looper.getMainLooper())
  private lateinit var status: TextView
  private lateinit var runButton: Button

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    buildContent()
    if (intent.getBooleanExtra("runProbe", false)) {
      startProbe()
    }
  }

  override fun onDestroy() {
    executor.shutdown()
    super.onDestroy()
  }

  private fun buildContent() {
    status = TextView(this).apply {
      text = "Ready. Artifacts are packaged offline in this APK."
      textSize = 17f
      setPadding(32, 32, 32, 32)
    }
    runButton = Button(this).apply {
      text = "Run cold k=15 Zswap spend proof"
      setOnClickListener { startProbe() }
    }
    setContentView(LinearLayout(this).apply {
      orientation = LinearLayout.VERTICAL
      gravity = Gravity.CENTER_HORIZONTAL
      addView(status)
      addView(runButton)
    })
  }

  private fun startProbe() {
    if (!running.compareAndSet(false, true)) return
    runButton.isEnabled = false
    status.text = "Mapping artifacts and proving…"
    Log.i(LOG_TAG, "PROBE_START epochMs=${System.currentTimeMillis()}")
    executor.execute {
      val outcome = runCatching { executeProbe() }
      outcome.onFailure { error ->
        Log.e(LOG_TAG, "PROBE_RESULT status=CRASH message=${error.javaClass.simpleName}", error)
      }
      mainHandler.post {
        status.text = outcome.fold(
          onSuccess = { it },
          onFailure = { "Failed: ${it.javaClass.simpleName}" },
        )
        runButton.isEnabled = true
        running.set(false)
      }
    }
  }

  private fun executeProbe(): String {
    val artifacts = listOf(
      mapAsset("request.bin"),
      mapAsset("bls_midnight_2p15"),
      mapAsset("zswap/9/spend.prover"),
      mapAsset("zswap/9/spend.verifier"),
      mapAsset("zswap/9/spend.bzkir"),
    )
    val inputs = inputs(artifacts)
    val result = ProbeResult.ByReference()
    inputs.write()
    result.write()
    val native = Native.load("midnight_native_runtime", ProverNative::class.java)
    val code = synchronized(artifacts) {
      native.midnight_embedded_prover_probe(inputs, result)
    }
    result.read()
    if (code != 0) {
      Log.i(LOG_TAG, "PROBE_RESULT status=ERROR code=$code")
      return "Native error $code"
    }
    val pointer = checkNotNull(result.taggedProofBytes)
    try {
      val size = Math.toIntExact(result.taggedProofBytesLen)
      val proof = pointer.getByteArray(0, size)
      val proofFile = File(filesDir, "proof-v2.bin")
      try {
        FileOutputStream(proofFile).use { it.write(proof) }
      } finally {
        proof.fill(0)
      }
      Log.i(
        LOG_TAG,
        "PROBE_RESULT status=SUCCESS proofSize=$size " +
          "artifactDecodingMs=${result.artifactDecodingMillis} " +
          "provingMs=${result.provingAndSelfVerificationMillis} " +
          "proofPath=${proofFile.absolutePath}",
      )
      return "Success: $size-byte tagged V2 proof\n" +
        "Decode ${result.artifactDecodingMillis} ms; prove+verify " +
        "${result.provingAndSelfVerificationMillis} ms"
    } finally {
      native.midnight_embedded_prover_free(pointer, result.taggedProofBytesLen)
    }
  }

  private fun inputs(artifacts: List<MappedArtifact>): ProbeInputs.ByReference {
    require(artifacts.size == 5)
    return ProbeInputs.ByReference().apply {
      request = Native.getDirectBufferPointer(artifacts[0].bytes)
      requestLen = artifacts[0].bytes.capacity().toLong()
      paramsK15 = Native.getDirectBufferPointer(artifacts[1].bytes)
      paramsK15Len = artifacts[1].bytes.capacity().toLong()
      proverKey = Native.getDirectBufferPointer(artifacts[2].bytes)
      proverKeyLen = artifacts[2].bytes.capacity().toLong()
      verifierKey = Native.getDirectBufferPointer(artifacts[3].bytes)
      verifierKeyLen = artifacts[3].bytes.capacity().toLong()
      ir = Native.getDirectBufferPointer(artifacts[4].bytes)
      irLen = artifacts[4].bytes.capacity().toLong()
    }
  }

  private fun mapAsset(name: String): MappedArtifact {
    val descriptor = assets.openFd(name)
    descriptor.use {
      FileInputStream(it.fileDescriptor).channel.use { channel ->
        val mapped = channel.map(
          FileChannel.MapMode.READ_ONLY,
          it.startOffset,
          it.declaredLength,
        ).asReadOnlyBuffer()
        require(mapped.isDirect) { "$name was not mapped as a direct buffer" }
        return MappedArtifact(name, mapped)
      }
    }
  }
}
