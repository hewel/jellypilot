package io.github.hewel.jellypilot

import android.app.KeyguardManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.PowerManager
import androidx.annotation.MainThread
import androidx.core.content.ContextCompat

/** Activity visibility, not window focus: an unfocused split-screen window remains eligible. */
@MainThread
internal class PlaybackVisibility(context: Context, private val changed: (Boolean) -> Unit) : AutoCloseable {
  private val application = context.applicationContext
  private val keyguard = application.getSystemService(KeyguardManager::class.java)
  private val power = application.getSystemService(PowerManager::class.java)
  private var visible = false
  private var screenOff = !power.isInteractive
  private var closed = false

  val eligible: Boolean
    get() = !closed && visible && !screenOff && power.isInteractive && !keyguard.isKeyguardLocked

  private val receiver = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
      when (intent.action) {
        Intent.ACTION_SCREEN_OFF -> screenOff = true
        Intent.ACTION_SCREEN_ON, Intent.ACTION_USER_PRESENT -> screenOff = !power.isInteractive
      }
      changed(eligible)
    }
  }

  init {
    ContextCompat.registerReceiver(
      application,
      receiver,
      IntentFilter().apply {
        addAction(Intent.ACTION_SCREEN_OFF)
        addAction(Intent.ACTION_SCREEN_ON)
        addAction(Intent.ACTION_USER_PRESENT)
      },
      ContextCompat.RECEIVER_NOT_EXPORTED,
    )
  }

  fun setVisible(value: Boolean) {
    if (closed) return
    visible = value
    screenOff = !power.isInteractive
    // Becoming eligible only permits a new explicit command; it never resumes playback.
    changed(eligible)
  }

  override fun close() {
    if (closed) return
    closed = true
    changed(false)
    application.unregisterReceiver(receiver)
  }
}
