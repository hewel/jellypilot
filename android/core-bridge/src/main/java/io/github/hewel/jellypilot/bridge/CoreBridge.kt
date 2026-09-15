package io.github.hewel.jellypilot.bridge

import android.content.Context
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ffi.SdkConfig
import io.github.hewel.jellypilot.ffi.SdkHooks

/**
 * Entry point for the shared business SDK on Android.
 *
 * Owns the [JellypilotSdk] handle for the application process. Create once
 * from [android.app.Application.onCreate] (or a DI singleton), call
 * [JellypilotSdk.shutdown] before disposing its generated handle, and never
 * create one per composition or per screen.
 */
object CoreBridge {
  /**
   * Creates the SDK bound to this app's private storage and the
   * Keystore-protected credential adapter.
   *
   * @param hooks optional platform teardown seam invoked while the previous
   *   profile is still active during activation, disconnect, and sign-out.
   *   Returning `false` declines activation or disconnect. Sign-out has already
   *   deleted protected credentials and reports cleanup failure in its outcome.
   */
  fun create(context: Context, deviceName: String, hooks: SdkHooks? = null): JellypilotSdk {
    val appContext = context.applicationContext
    return JellypilotSdk(
      config = SdkConfig(
        storageDir = appContext.filesDir.absolutePath,
        deviceName = deviceName,
      ),
      credentialStore = KeystoreCredentialStore(appContext),
      hooks = hooks,
    )
  }
}
