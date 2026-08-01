package dev.oneam.midnightmobile.localprover

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

  private fun proverBridge(): LocalProverBridge = synchronized(this) {
    bridge ?: LocalProverBridge(appContext.reactContext?.assets).also { bridge = it }
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

    AsyncFunction("close") {
      try {
        bridge?.close()
      } catch (error: Throwable) {
        throw localProverException(error, "NATIVE_INTERNAL")
      }
    }

    OnDestroy {
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
