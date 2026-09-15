package io.github.hewel.jellypilot

import android.app.Application
import android.content.Context
import android.os.Build
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import io.github.hewel.jellypilot.bridge.CoreBridge
import io.github.hewel.jellypilot.ffi.SdkHooks
import coil3.ImageLoader
import coil3.SingletonImageLoader
import coil3.disk.DiskCache
import coil3.memory.MemoryCache
import okhttp3.OkHttpClient
import okio.Path.Companion.toOkioPath

class JellyPilotApplication : Application(), SingletonImageLoader.Factory, ViewModelStoreOwner {
  override val viewModelStore = ViewModelStore()
  private val playerInstance = lazy { NativePlayback(this) }
  internal val player get() = playerInstance.value
  private val sdkInstance = lazy {
    CoreBridge.create(this, Build.MODEL, object : SdkHooks {
      override suspend fun beforeProfileHandoff(): Boolean {
        player.setHandoffBlocked(true)
        return player.stopAndWait()
      }
    })
  }
  internal val sdk get() = sdkInstance.value

  override fun onTerminate() {
    viewModelStore.clear()
    if (sdkInstance.isInitialized()) {
      sdkInstance.value.shutdown()
      sdkInstance.value.close()
    }
    if (playerInstance.isInitialized()) playerInstance.value.close()
    super.onTerminate()
  }

  override fun newImageLoader(context: Context): ImageLoader = ImageLoader.Builder(context)
    .memoryCache { MemoryCache.Builder().maxSizePercent(context, 0.20).build() }
    .diskCache {
      // Coil owns original network bytes as well as decode/memory caching; Rust only supplies references.
      DiskCache.Builder().directory(context.cacheDir.resolve("artwork").toOkioPath())
        .maxSizeBytes(512L * 1024L * 1024L).build()
    }
    .components {
      add(ArtworkScopeInterceptor { sdk })
      add(ArtworkFetcherFactory { sdk })
    }
    .build()
}

// Match the shared client's no-redirect image transport, including same-origin base-path boundaries.
internal fun artworkHttpClient(): OkHttpClient = OkHttpClient.Builder()
  .followRedirects(false)
  .followSslRedirects(false)
  .build()
