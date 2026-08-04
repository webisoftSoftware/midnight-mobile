package dev.oneam.midnightmobile.localprover

import android.app.ActivityManager
import android.content.ComponentCallbacks2
import android.content.Context
import android.content.res.Configuration
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import kotlin.concurrent.thread

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
    }

    AsyncFunction("check") { request: ByteArray ->
      try {
        proverBridge().check(request)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      } finally {
        request.fill(0)
      }
    }

    AsyncFunction("prove") { request: ByteArray ->
      try {
        proverBridge().prove(request)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      } finally {
        request.fill(0)
      }
    }

    AsyncFunction("proveBatch") { requests: List<ByteArray> ->
      try {
        proverBridge().proveBatch(requests)
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      } finally {
        requests.forEach { it.fill(0) }
      }
    }

    AsyncFunction("cancel") {
      bridge?.cancel()
    }

    AsyncFunction("close") {
      try {
        bridge?.close()
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      }
    }

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
