package io.github.hewel.jellypilot.player

import android.content.Context
import android.util.AttributeSet
import android.view.SurfaceHolder
import android.view.SurfaceView

/**
 * A [SurfaceView] that binds its surface to a [PlayerHost].
 *
 * `surfaceDestroyed` calls [PlayerHost.detachSurface], which blocks until
 * libmpv has released the underlying ANativeWindow — so by the time the
 * callback returns, the surface may safely be destroyed by the framework.
 * The host instance and playback survive detach/reattach cycles (rotation,
 * window changes); only [PlayerHost.release] ends them.
 *
 * Usage from Compose: `AndroidView(factory = { MpvSurfaceView(it).apply { attach(host) } })`.
 */
class MpvSurfaceView @JvmOverloads constructor(
  context: Context,
  attrs: AttributeSet? = null,
) : SurfaceView(context, attrs), SurfaceHolder.Callback {

  private var host: PlayerHost? = null

  init {
    holder.addCallback(this)
  }

  /** Binds [host]; a previously bound host is detached first. */
  fun attach(host: PlayerHost) {
    if (this.host === host) return
    this.host?.detachSurface()
    this.host = host
    if (holder.surface.isValid) {
      host.attachSurface(holder.surface)
    }
  }

  /** Unbinds the host without releasing it; playback continues without video. */
  fun detach() {
    host?.detachSurface()
    host = null
  }

  override fun surfaceCreated(holder: SurfaceHolder) {
    host?.attachSurface(holder.surface)
  }

  override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
    host?.setSurfaceSize(width, height)
  }

  override fun surfaceDestroyed(holder: SurfaceHolder) {
    // Blocks until native relinquishment; safe for the surface to die after.
    host?.detachSurface()
  }
}
