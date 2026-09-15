package io.github.hewel.jellypilot.player

import android.content.Context
import android.view.Surface

/** Thrown when native player creation or a required native step fails. */
class PlayerHostException(message: String) : RuntimeException(message)

/**
 * JNI boundary to the libmpv host. Internal: consumers use [MpvPlayerHost].
 *
 * All functions take the instance handle returned by [nativeCreate]. Calls are
 * serialized by [MpvPlayerHost]'s executor; the native side additionally
 * tolerates calls from any thread except inside listener callbacks.
 */
internal object MpvJni {
  init {
    // libmpv must be resolvable before the JNI library links against it.
    System.loadLibrary("mpv")
    System.loadLibrary("jellypilot_player")
  }

  /** Receives mpv events on the native event thread. Implementations must only enqueue. */
  internal interface NativeListener {
    fun onPropertyFlag(name: String, value: Int)
    fun onPropertyLong(name: String, value: Long)
    fun onPropertyDouble(name: String, value: Double)
    fun onPropertyString(name: String, value: String)
    fun onPropertyNone(name: String)
    fun onEvent(eventId: Int)

    /** MPV_EVENT_START_FILE: [playlistEntryId] is the id returned by [nativeLoadFile]. */
    fun onStartFile(playlistEntryId: Long)

    /**
     * MPV_EVENT_END_FILE: [playlistEntryId] matches the START_FILE id,
     * [reason] is mpv_end_file_reason, [error] an mpv_error code.
     */
    fun onEndFile(playlistEntryId: Long, reason: Int, error: Int)

    fun onLog(prefix: String, level: Int, text: String)
  }

  @JvmStatic
  external fun nativeCreate(
    appContext: Context,
    listener: NativeListener,
    cacheDir: String?,
    tlsCaFile: String?,
  ): Long

  @JvmStatic
  external fun nativeDestroy(handle: Long)

  @JvmStatic
  external fun nativeAttachSurface(handle: Long, surface: Surface)

  /** Blocks until libmpv has released the ANativeWindow (see C++ docs). */
  @JvmStatic
  external fun nativeDetachSurface(handle: Long)

  /** Runs a synchronous mpv command; returns an mpv_error code (0 = success). */
  @JvmStatic
  external fun nativeCommand(handle: Long, args: Array<String>): Int

  /**
   * Runs a synchronous `loadfile` command via mpv_command_ret and returns the
   * accepted entry's `playlist_entry_id` (> 0). A negative value is an
   * mpv_error code: the command was refused and no entry was installed.
   *
   * The id correlates this load with the START_FILE and END_FILE events that
   * carry the same `playlist_entry_id`. An accepted entry that is stopped or
   * replaced before mpv starts it never emits either event.
   */
  @JvmStatic
  external fun nativeLoadFile(handle: Long, args: Array<String>): Long

  @JvmStatic
  external fun nativeSetPropertyString(handle: Long, name: String, value: String): Int

  @JvmStatic
  external fun nativeSetPropertyFlag(handle: Long, name: String, value: Boolean): Int

  @JvmStatic
  external fun nativeSetPropertyDouble(handle: Long, name: String, value: Double): Int

  @JvmStatic
  external fun nativeGetPropertyString(handle: Long, name: String): String?

  /** Replaces `http-header-fields` with the given `Name: value` lines (empty clears). */
  @JvmStatic
  external fun nativeSetHttpHeaders(handle: Long, headerLines: Array<String>): Int

  /**
   * Queues a no-op command on the mpv core thread and blocks until its reply
   * arrives, proving every previously queued async change was processed.
   */
  @JvmStatic
  external fun nativeBarrier(handle: Long, timeoutMs: Int): Boolean
  /** [format] is an mpv_format value (0 none, 1 string, 3 flag, 4 int64, 5 double). */
  @JvmStatic
  external fun nativeObserveProperty(handle: Long, name: String, format: Int)
}
