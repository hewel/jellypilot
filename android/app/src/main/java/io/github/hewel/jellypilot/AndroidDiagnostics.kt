package io.github.hewel.jellypilot

import android.app.Activity
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.content.FileProvider
import coil3.SingletonImageLoader
import io.github.hewel.jellypilot.player.PlayerStatus
import java.io.File
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** No free-form server/player strings: URLs, credentials and media names cannot enter an export. */
internal data class AndroidDiagnosticState(
  val signedIn: Boolean,
  val savedProfiles: Int,
  val playbackStatus: PlayerStatus,
  val playerReady: Boolean,
  val recoveryAvailable: Boolean,
  val playbackError: Boolean,
  val reportingError: Boolean,
)

internal fun androidDiagnosticReport(state: AndroidDiagnosticState): String = buildString {
  appendLine("JellyPilot Android diagnostics")
  appendLine("app_version=${BuildConfig.VERSION_NAME}")
  appendLine("version_code=${BuildConfig.VERSION_CODE}")
  appendLine("android_api=${Build.VERSION.SDK_INT}")
  appendLine("signed_in=${state.signedIn}")
  appendLine("saved_profiles=${state.savedProfiles.coerceAtLeast(0)}")
  appendLine("playback_status=${state.playbackStatus.name}")
  appendLine("player_ready=${state.playerReady}")
  appendLine("recovery_available=${state.recoveryAvailable}")
  appendLine("playback_error=${state.playbackError}")
  appendLine("reporting_error=${state.reportingError}")
}

/** Produces a scoped read grant; choosing a destination remains an explicit Android user action. */
internal suspend fun exportAndroidDiagnostics(context: Context, state: AndroidDiagnosticState) {
  val report = androidDiagnosticReport(state)
  val uri = withContext(Dispatchers.IO) {
    val directory = context.cacheDir.resolve("diagnostics")
    if (!directory.isDirectory && !directory.mkdirs()) throw IOException("Could not create diagnostics directory")
    // These are the exporter's own temporary files, never player or credential storage.
    val oldest = System.currentTimeMillis() - 24 * 60 * 60 * 1000L
    directory.listFiles()?.filter { it.isFile && it.name.startsWith("jellypilot-") && it.lastModified() < oldest }
      ?.forEach { it.delete() }
    val file = File.createTempFile("jellypilot-", ".txt", directory)
    file.writeText(report)
    FileProvider.getUriForFile(context, "${context.packageName}.diagnostics", file)
  }
  withContext(Dispatchers.Main.immediate) {
    val send = Intent(Intent.ACTION_SEND).apply {
      type = "text/plain"
      putExtra(Intent.EXTRA_STREAM, uri)
      clipData = ClipData.newRawUri("diagnostics", uri)
      addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    val chooser = Intent.createChooser(send, context.localizedString(R.string.diagnostics))
    if (context !is Activity) chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    context.startActivity(chooser)
  }
}

/** Coil owns artwork bytes; clearing them must never delete SDK files, credentials or recovery. */
internal suspend fun clearArtworkCache(context: Context) = withContext(Dispatchers.IO) {
  val loader = SingletonImageLoader.get(context)
  loader.memoryCache?.clear()
  loader.diskCache?.clear()
  Unit
}
