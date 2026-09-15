package io.github.hewel.jellypilot

import coil3.Extras
import coil3.ImageLoader
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.intercept.Interceptor
import coil3.request.ImageResult
import coil3.network.NetworkHeaders
import coil3.network.httpHeaders
import coil3.network.okhttp.OkHttpNetworkFetcherFactory
import coil3.request.Options
import coil3.toUri
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ui.ArtworkUi
import java.io.IOException

/** Resolves credentials only inside Coil's fetch operation, never in the Compose request or state. */
internal class ArtworkFetcherFactory(private val sdk: () -> JellypilotSdk) : Fetcher.Factory<ArtworkUi> {
  private val network = OkHttpNetworkFetcherFactory(callFactory = { artworkHttpClient() })

  override fun create(data: ArtworkUi, options: Options, imageLoader: ImageLoader): Fetcher = object : Fetcher {
    override suspend fun fetch(): FetchResult? {
      val core = sdk()
      val target = core.imageTarget(data.scope, data.imageId, 384u)
      val headers = NetworkHeaders.Builder()
        .set("Authorization", target.authorization)
        .set("User-Agent", target.userAgent)
        .set("Accept", target.accept)
        .build()
      val securedOptions = options.copy(extras = options.extras.newBuilder().set(Extras.Key.httpHeaders, headers).build())
      return requireNotNull(network.create(target.url.toUri(), securedOptions, imageLoader)).fetch()
    }
  }
}

/** Enforce scope even when Coil can satisfy the request from memory or disk. */
internal class ArtworkScopeInterceptor(private val sdk: () -> JellypilotSdk) : Interceptor {
  override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
    val artwork = chain.request.data as? ArtworkUi ?: return chain.proceed()
    val core = sdk()
    if (!core.isScopeActive(artwork.scope)) throw IOException("Artwork scope is no longer active")
    val result = chain.proceed()
    if (!core.isScopeActive(artwork.scope)) throw IOException("Artwork scope ended during fetch")
    return result
  }
}
