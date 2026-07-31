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
import expo.modules.midnightlocalprover.LocalProverBridge
import expo.modules.midnightlocalprover.LocalProverCircuit
import expo.modules.midnightlocalprover.LocalProverConfiguration
import expo.modules.midnightlocalprover.LocalProverFile
import expo.modules.midnightlocalprover.LocalProverParameter
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
      text = "Run local /check and k=15 Zswap spend /prove"
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

      val proveStarted = System.nanoTime()
      val proveRequest = assets.open("request.bin").use { it.readBytes() }
      val proof = try {
        bridge.prove(proveRequest)
      } finally {
        proveRequest.fill(0)
      }
      val proveMillis = elapsedMillis(proveStarted)
      writeResult("proof-v2.bin", proof)
      Log.i(
        LOG_TAG,
        "PROBE_RESULT status=SUCCESS proofSize=${proof.size} checkSize=${check.size} " +
          "configurationMs=$configurationMillis checkMs=$checkMillis provingMs=$proveMillis " +
          "proofPath=${File(filesDir, "proof-v2.bin").absolutePath}",
      )
      "Success: ${proof.size}-byte tagged V2 proof\n" +
        "Configure $configurationMillis ms; check $checkMillis ms; prove+verify $proveMillis ms"
    } finally {
      bridge.close()
    }
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
