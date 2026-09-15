package io.github.hewel.jellypilot

import android.content.Context
import java.io.File

internal object TestMedia {
  const val SAMPLE_ASSET = "sample.mkv"
  const val SUBTITLE_ASSET = "sample.eng.srt"

  fun stage(assets: Context, asset: String, directory: File = File(assets.cacheDir, "test-media")): File {
    check(directory.mkdirs() || directory.isDirectory)
    val target = File(directory, asset.substringAfterLast('/'))
    assets.assets.open(asset).use { input -> target.outputStream().use(input::copyTo) }
    return target
  }

  fun tree(assets: Context, asset: String, directory: File): File {
    val target = File(directory, asset.substringAfterLast('/')).apply { check(mkdirs() || isDirectory) }
    for (name in requireNotNull(assets.assets.list(asset))) {
      val path = "$asset/$name"
      if (assets.assets.list(path).orEmpty().isEmpty()) stage(assets, path, target)
      else tree(assets, path, target)
    }
    return target
  }
}
