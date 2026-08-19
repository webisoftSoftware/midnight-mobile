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
import dev.oneam.midnightmobile.localprover.LocalProverBridge
import dev.oneam.midnightmobile.localprover.LocalProverCircuit
import dev.oneam.midnightmobile.localprover.LocalProverConfiguration
import dev.oneam.midnightmobile.localprover.LocalProverFile
import dev.oneam.midnightmobile.localprover.LocalProverParameter
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

private const val LOG_TAG = "EmbeddedProverSpike"

private fun asset(uri: String, size: Long, sha256: String) = LocalProverFile(uri, size, sha256)

private val PROVER_CONFIGURATION = LocalProverConfiguration(
  parameters = listOf(
    LocalProverParameter(
      15,
      asset(
        "asset://bls_midnight_2p15",
        6_291_844,
        "724c7c3d779148bb113c7ee9c034b2f27db16e6bdf315fde90105a9bad00b1de",
      ),
    ),
    LocalProverParameter(
      14,
      asset(
        "asset://bls_midnight_2p14",
        3_146_116,
        "fc253016885ec830e97808c9ec920bb5cab5c21af590380a6cb5eb0538e2b244",
      ),
    ),
  ),
  circuits = listOf(
    LocalProverCircuit(
      keyLocation = "midnight/zswap/spend",
      proverKey = asset(
        "asset://zswap/9/spend.prover",
        11_020_001,
        "19d234b5c68b7212ad6b0ec9334a95594748154128f3704eb576bcc843cc5c45",
      ),
      verifierKey = asset(
        "asset://zswap/9/spend.verifier",
        2_311,
        "544554effd7ae9fb9063be52a9ec2a986756301071fcd97bb4598fb45a335658",
      ),
      ir = asset(
        "asset://zswap/9/spend.bzkir",
        1_294,
        "7cb5bbcf67cb212a3336fb439a77e8f32f0aa8a56185c8e1247d6cbfc7300205",
      ),
    ),
    LocalProverCircuit(
      keyLocation = "midnight/zswap/output",
      proverKey = asset(
        "asset://zswap/9/output.prover",
        5_730_182,
        "d992b04f13c3fd432f55fb8bfe6466d87bc181f1a2acf233ec228030bbdd4ed8",
      ),
      verifierKey = asset(
        "asset://zswap/9/output.verifier",
        2_311,
        "72e8074856f2f5c504ade25a86a2b8902c64aeb9497c4c8e6b26dea842a0ab08",
      ),
      ir = asset(
        "asset://zswap/9/output.bzkir",
        494,
        "91dc8b401dd8385e8d29eaac018c70b578505f48c7952452ef319bc397fa1f1b",
      ),
    ),
  ),
)

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
    if (intent.getBooleanExtra("runProbe", false)) startProbe()
  }

  override fun onDestroy() {
    executor.shutdown()
    super.onDestroy()
  }

  private fun buildContent() {
    status = TextView(this).apply {
      text = "Ready. The SDK local-prover bridge will map packaged artifacts offline."
      textSize = 17f
      setPadding(32, 32, 32, 32)
    }
    runButton = Button(this).apply {
      text = "Run local /check plus k=15 spend and k=14 output /prove"
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
    status.text = "Configuring, checking, and proving…"
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
    val bridge = LocalProverBridge(assets)
    // Measurement controls, driven by intent extras so one APK covers every run in
    // a measurement matrix. `maxConcurrency` is the admission limit: at the default
    // a batched proof shares one four-thread pool with its neighbour, so a stage
    // total cannot be attributed to the circuit until it is pinned to 1.
    val maxConcurrency = intent.getIntExtra("maxConcurrency", 0)
    if (maxConcurrency > 0) {
      bridge.setMaxConcurrency(maxConcurrency)
      Log.i(LOG_TAG, "PROBE_CONFIG maxConcurrency=$maxConcurrency")
    }
    val profiling = intent.getBooleanExtra("profiling", false)
    if (profiling) {
      bridge.setProfiling(true)
      // Drain anything a previous run left queued so the samples below belong to
      // this sequence only.
      bridge.takeTimings()
    }
    val batch = intent.getBooleanExtra("batch", false)
    val configuredAt = System.nanoTime()
    bridge.configure(PROVER_CONFIGURATION)
    val configurationMillis = elapsedMillis(configuredAt)
    return try {
      val checkStarted = System.nanoTime()
      val checkRequest = assets.open("check-request.bin").use { it.readBytes() }
      val check = try {
        bridge.check(checkRequest)
      } finally {
        checkRequest.fill(0)
      }
      val checkMillis = elapsedMillis(checkStarted)
      writeResult("check-response.bin", check)

      if (batch) {
        // The shape behind the batch panic: several proofs admitted concurrently
        // into one pool. A spend plus two outputs is what a real send emits.
        val batchStarted = System.nanoTime()
        val batchRequests = listOf("request.bin", "output-request.bin", "output-request.bin")
          .map { name -> assets.open(name).use { it.readBytes() } }
        val batchProofs = try {
          bridge.proveBatch(batchRequests)
        } finally {
          for (request in batchRequests) request.fill(0)
        }
        val batchMillis = elapsedMillis(batchStarted)
        val batchSizes = batchProofs.map { it.size }
        // Write the first two proofs under the names the host runner validates, and
        // emit PROBE_RESULT as well as PROBE_BATCH: PROBE_RESULT is the runner's
        // terminal-line contract, and without it a batch run is indistinguishable
        // from a hang and burns the full 15-minute timeout.
        writeResult("proof-v2.bin", batchProofs[0])
        writeResult("output-proof-v2.bin", batchProofs[1])
        Log.i(
          LOG_TAG,
          "PROBE_BATCH status=SUCCESS proofs=${batchProofs.size} " +
            "sizes=${batchSizes.joinToString(",")} ms=$batchMillis",
        )
        if (profiling) logTimings(bridge)
        Log.i(
          LOG_TAG,
          "PROBE_RESULT status=SUCCESS proofSize=${batchSizes[0]} checkSize=${check.size} " +
            "configurationMs=$configurationMillis checkMs=$checkMillis provingMs=$batchMillis " +
            "outputProofSize=${batchSizes[1]} outputProvingMs=$batchMillis " +
            "proofPath=${File(filesDir, "proof-v2.bin").absolutePath} " +
            "outputProofPath=${File(filesDir, "output-proof-v2.bin").absolutePath}",
        )
        return "Batch success: ${batchProofs.size} proofs in $batchMillis ms"
      }

      val proveStarted = System.nanoTime()
      val proveRequest = assets.open("request.bin").use { it.readBytes() }
      val proof = try {
        bridge.prove(proveRequest)
      } finally {
        proveRequest.fill(0)
      }
      val proveMillis = elapsedMillis(proveStarted)
      writeResult("proof-v2.bin", proof)

      val outputProveStarted = System.nanoTime()
      val outputProveRequest = assets.open("output-request.bin").use { it.readBytes() }
      val outputProof = try {
        bridge.prove(outputProveRequest)
      } finally {
        outputProveRequest.fill(0)
      }
      val outputProveMillis = elapsedMillis(outputProveStarted)
      writeResult("output-proof-v2.bin", outputProof)
      Log.i(
        LOG_TAG,
        "PROBE_RESULT status=SUCCESS proofSize=${proof.size} checkSize=${check.size} " +
          "configurationMs=$configurationMillis checkMs=$checkMillis provingMs=$proveMillis " +
          "outputProofSize=${outputProof.size} outputProvingMs=$outputProveMillis " +
          "proofPath=${File(filesDir, "proof-v2.bin").absolutePath} " +
          "outputProofPath=${File(filesDir, "output-proof-v2.bin").absolutePath}",
      )
      if (profiling) logTimings(bridge)
      "Success: ${proof.size}-byte spend and ${outputProof.size}-byte output tagged V2 proofs\n" +
        "Configure $configurationMillis ms; check $checkMillis ms; " +
        "spend $proveMillis ms; output $outputProveMillis ms"
    } finally {
      bridge.close()
    }
  }

  /**
   * Drains the native per-proof stage samples. With the instrumentation rig staged
   * (see `tools/mobile-prover-instrumentation`) each sample also carries
   * `phaseCounters`, which is what decomposes the proof's uncounted remainder.
   * Logged as one line per drain so `adb logcat` is the whole transport.
   */
  private fun logTimings(bridge: LocalProverBridge) {
    val timings = runCatching { bridge.takeTimings() }
      .getOrElse { error -> "\"${error.javaClass.simpleName}\"" }
    Log.i(LOG_TAG, "PROBE_TIMINGS $timings")
  }

  private fun elapsedMillis(started: Long): Long = (System.nanoTime() - started) / 1_000_000

  private fun writeResult(name: String, bytes: ByteArray) {
    try {
      FileOutputStream(File(filesDir, name)).use { it.write(bytes) }
    } finally {
      bytes.fill(0)
    }
  }
}
