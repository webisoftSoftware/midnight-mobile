package dev.oneam.midnightmobile.localprover

import android.app.ActivityManager
import android.content.ComponentCallbacks2
import android.content.Context
import android.content.res.Configuration
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher

// Matches the native MAX_CONCURRENT_PROOFS_CEILING: admission is still decided in Rust, this only
// has to be wide enough not to become the narrower bound.
private const val PROVER_QUEUE_THREADS = 4

// Declared before the queue that uses it: top-level property initializers run in file order.
private val threadCounter = AtomicInteger(0)

/**
 * A queue of this module's own, and not Expo's default.
 *
 * Every Expo `AsyncFunction` body otherwise runs on the single
 * `expo.modules.AsyncFunctionQueue` HandlerThread shared by every module in the app. A proof holds
 * that thread for seconds — around 15 s for a two-proof round on the measured device — which
 * blocks every other module's async calls for the duration and serializes two `prove` calls that
 * the caller asked to overlap. iOS's module has always declared a concurrent queue; this is the
 * Android counterpart.
 */
private val localProverQueue = CoroutineScope(
  Executors.newFixedThreadPool(PROVER_QUEUE_THREADS) { runnable ->
    thread(start = false, name = "midnight-local-prover-${threadCounter.incrementAndGet()}") {
      runnable.run()
    }
  }.asCoroutineDispatcher() +
    SupervisorJob() +
    CoroutineName("midnight.localProverQueue")
)

private fun localProverException(error: Throwable, fallbackCode: String): CodedException {
  val code = (error as? LocalProverBridgeException)?.stableCode ?: fallbackCode
  return CodedException(code, code, error)
}

class MidnightMobileLocalProverModule : Module() {
  private var bridge: LocalProverBridge? = null
  private var trimMemoryCallbacks: ComponentCallbacks2? = null

  private fun proverBridge(): LocalProverBridge = synchronized(this) {
    bridge ?: LocalProverBridge(appContext.reactContext?.assets, isLowRamDevice()).also {
      bridge = it
    }
  }

  private fun isLowRamDevice(): Boolean {
    val activityManager = appContext.reactContext
      ?.getSystemService(Context.ACTIVITY_SERVICE) as? ActivityManager
    return activityManager?.isLowRamDevice ?: false
  }

  override fun definition() = ModuleDefinition {
    Name("MidnightMobileLocalProver")

    AsyncFunction("configure") { configurationJson: String ->
      try {
        proverBridge()
          .configure(LocalProverBridge.decodeConfiguration(configurationJson))
          .toDouble()
      } catch (error: Throwable) {
        throw localProverException(error, "INVALID_CONFIGURATION")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("check") { request: ByteArray ->
      try {
        proverBridge().check(request)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      } finally {
        request.fill(0)
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("prove") { request: ByteArray ->
      try {
        proverBridge().prove(request)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      } finally {
        request.fill(0)
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("proveBatch") { requests: List<ByteArray> ->
      try {
        proverBridge().proveBatch(requests)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      } finally {
        requests.forEach { it.fill(0) }
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("cancel") {
      bridge?.cancel()
    }.runOnQueue(localProverQueue)

    // Instrumentation, not proving: both are process-wide, take no registry handle, and stay
    // callable when no registry is configured, so a drain still reports the last proof of a
    // session after its registry has been closed.
    AsyncFunction("setProfiling") { enabled: Boolean ->
      try {
        proverBridge().setProfiling(enabled)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("takeTimings") {
      try {
        proverBridge().takeTimings()
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    // Pinning the admission limit is what makes a proving measurement attributable: at the
    // default limit a batched proof shares one four-thread pool with its neighbour, so its
    // duration cannot be split between the circuit and the crowding. The JS layer rejects limits
    // outside 1..4 before they reach here, so the requested limit is the applied one.
    AsyncFunction("setMaxConcurrency") { limit: Int ->
      try {
        proverBridge().setMaxConcurrency(limit)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    AsyncFunction("close") {
      try {
        bridge?.close()
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      }
    }.runOnQueue(localProverQueue)

    OnCreate {
      // Phase 5 memory-pressure hook: Expo's module DSL has no dedicated trim-memory event, so
      // this registers the plain Android ComponentCallbacks2.onTrimMemory hook directly on the
      // application context, which is the standard native signal for this. Drops the admission
      // limit to 1 (refusing new admissions above that) rather than eagerly restoring a higher
      // limit when memory pressure eases, since there is no corresponding "pressure relieved"
      // callback to restore on.
      val callbacks = object : ComponentCallbacks2 {
        override fun onConfigurationChanged(newConfig: Configuration) = Unit

        override fun onLowMemory() {
          bridge?.setMaxConcurrency(1)
        }

        override fun onTrimMemory(level: Int) {
          if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_MODERATE) {
            bridge?.setMaxConcurrency(1)
          }
        }
      }
      appContext.reactContext?.applicationContext?.registerComponentCallbacks(callbacks)
      trimMemoryCallbacks = callbacks
    }

    OnDestroy {
      trimMemoryCallbacks?.let { callbacks ->
        appContext.reactContext?.applicationContext?.unregisterComponentCallbacks(callbacks)
      }
      trimMemoryCallbacks = null
      val current = synchronized(this@MidnightMobileLocalProverModule) {
        bridge.also { bridge = null }
      }
      if (current != null) {
        thread(name = "midnight-local-prover-close") {
          runCatching { current.close() }
        }
      }
    }
  }
}
