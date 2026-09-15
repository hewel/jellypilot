import javax.xml.parsers.DocumentBuilderFactory
import org.w3c.dom.Element

plugins {
  alias(libs.plugins.android.application)
  alias(libs.plugins.compose.compiler)
}

abstract class BrandAssets : Sync() {
  @get:OutputDirectory abstract val outputDirectory: DirectoryProperty
}

abstract class GenerateIcons : DefaultTask() {
  @get:InputDirectory
  @get:PathSensitive(PathSensitivity.RELATIVE)
  abstract val sourceDirectory: DirectoryProperty
  @get:OutputDirectory abstract val outputDirectory: DirectoryProperty

  @TaskAction
  fun generate() {
    val destination = outputDirectory.dir("drawable").get().asFile
    destination.deleteRecursively()
    destination.mkdirs()
    val parser = DocumentBuilderFactory.newInstance().apply {
      setFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
    }.newDocumentBuilder()
    sourceDirectory.get().asFile.listFiles { file -> file.extension == "svg" }!!.sortedBy { it.name }.forEach { source ->
      val svg = parser.parse(source).documentElement
      require(svg.getAttribute("viewBox") == "0 0 24 24") { "Unexpected icon viewport: ${source.name}" }
      val paths = buildString {
        for (index in 0 until svg.childNodes.length) {
          val node = svg.childNodes.item(index) as? Element ?: continue
          val data = when (node.tagName) {
            "path" -> node.getAttribute("d")
            "polyline" -> {
              val points = node.getAttribute("points").trim().split(Regex("[ ,]+"))
              require(points.size >= 4 && points.size % 2 == 0) { "Invalid polyline: ${source.name}" }
              points.chunked(2).mapIndexed { point, xy -> "${if (point == 0) "M" else "L"}${xy[0]},${xy[1]}" }.joinToString(" ")
            }
            "line" -> "M${node.getAttribute("x1")},${node.getAttribute("y1")} L${node.getAttribute("x2")},${node.getAttribute("y2")}"
            "circle" -> {
              val cx = node.getAttribute("cx").toDouble()
              val cy = node.getAttribute("cy").toDouble()
              val radius = node.getAttribute("r").toDouble()
              "M${cx - radius},$cy a$radius,$radius 0 1,0 ${radius * 2},0 a$radius,$radius 0 1,0 ${-radius * 2},0"
            }
            else -> error("Unsupported icon geometry ${node.tagName}: ${source.name}")
          }
          val fill = node.getAttribute("fill").ifEmpty { svg.getAttribute("fill").ifEmpty { "currentColor" } }
          append("    <path android:pathData=\"$data\" android:fillColor=\"${if (fill == "none") "#00000000" else "#FF000000"}\"")
          if (node.getAttribute("fill-rule") == "evenodd") append(" android:fillType=\"evenOdd\"")
          if (node.hasAttribute("stroke") && node.getAttribute("stroke") != "none") {
            append(" android:strokeColor=\"#FF000000\" android:strokeWidth=\"${node.getAttribute("stroke-width").ifEmpty { "1" }}\"")
            if (node.hasAttribute("stroke-linecap")) append(" android:strokeLineCap=\"${node.getAttribute("stroke-linecap")}\"")
            if (node.hasAttribute("stroke-linejoin")) append(" android:strokeLineJoin=\"${node.getAttribute("stroke-linejoin")}\"")
          }
          append(" />\n")
        }
      }
      val name = source.nameWithoutExtension.replace('-', '_')
      val mirrored = name == "chevron_left" || name == "chevron_right"
      destination.resolve("ic_$name.xml").writeText(
        """<?xml version="1.0" encoding="utf-8"?>
<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24" android:autoMirrored="$mirrored">
$paths</vector>
""",
      )
    }
  }
}

val brandAssets = tasks.register<BrandAssets>("prepareBrandAssets") {
  from(rootProject.file("../crates/jellypilot-ui/assets/fonts")) { into("fonts") }
  from(rootProject.file("../crates/jellypilot-ui/assets/icons/LICENSE")) { into("icons") }
  outputDirectory.set(layout.buildDirectory.dir("generated/brandAssets"))
  into(outputDirectory)
}

val prepareIcons = tasks.register<GenerateIcons>("prepareIcons") {
  sourceDirectory.set(rootProject.file("../crates/jellypilot-ui/assets/icons"))
  outputDirectory.set(layout.buildDirectory.dir("generated/iconResources"))
}

// Synthetic media only: no private server files or credentials enter the test APK.
abstract class StageTestMedia : DefaultTask() {
  @get:Input abstract val ffmpeg: Property<String>
  @get:OutputDirectory abstract val outputDirectory: DirectoryProperty
  @get:javax.inject.Inject abstract val processes: ExecOperations

  @TaskAction
  fun stage() {
    val output = outputDirectory.get().asFile
    output.deleteRecursively()
    output.mkdirs()
    val subtitle = output.resolve("sample.eng.srt")
    subtitle.writeText("1\n00:00:00,000 --> 00:00:19,000\nJellyPilot native regression\n")
    val sample = output.resolve("sample.mkv")
    fun encode(vararg arguments: String) {
      processes.exec {
        commandLine(listOf(ffmpeg.get(), "-nostdin", "-hide_banner", "-loglevel", "error", "-y") + arguments)
      }
    }
    encode(
      "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=24",
      "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
      "-f", "lavfi", "-i", "sine=frequency=660:sample_rate=48000",
      "-i", subtitle.path, "-t", "20",
      "-map", "0:v", "-map", "1:a", "-map", "2:a", "-map", "3:s",
      "-c:v", "libx264", "-preset", "ultrafast", "-g", "48", "-pix_fmt", "yuv420p",
      "-c:a", "aac", "-c:s", "srt",
      "-metadata:s:a:0", "language=eng", "-metadata:s:a:1", "language=jpn",
      "-disposition:a:0", "default", "-disposition:a:1", "0", sample.path,
    )
    val hls = output.resolve("hls").apply { mkdirs() }
    encode("-i", sample.path, "-t", "8", "-map", "0:v:0", "-map", "0:a:0", "-c", "copy",
      "-hls_time", "2", "-hls_playlist_type", "vod", "-hls_segment_filename",
      hls.resolve("segment%03d.ts").path, hls.resolve("index.m3u8").path)
    val dash = output.resolve("dash").apply { mkdirs() }
    encode("-i", sample.path, "-t", "8", "-map", "0:v:0", "-map", "0:a:0", "-c", "copy",
      "-seg_duration", "2", "-f", "dash", dash.resolve("index.mpd").path)
  }
}

val stageTestMedia = tasks.register<StageTestMedia>("stageTestMedia") {
  ffmpeg.set(providers.gradleProperty("jellypilot.testFfmpeg").orElse("ffmpeg"))
  outputDirectory.set(layout.buildDirectory.dir("generated/testMedia"))
}

androidComponents.onVariants { variant ->
  variant.sources.assets?.addGeneratedSourceDirectory(brandAssets, BrandAssets::outputDirectory)
  variant.sources.res?.addGeneratedSourceDirectory(prepareIcons, GenerateIcons::outputDirectory)
  variant.androidTest?.sources?.assets?.addGeneratedSourceDirectory(stageTestMedia, StageTestMedia::outputDirectory)
}

android {
  namespace = "io.github.hewel.jellypilot"
  compileSdk = 37
  ndkVersion = "28.2.13676358"
  defaultConfig {
    applicationId = "io.github.hewel.jellypilot"
    minSdk = 26
    targetSdk = 37
    versionCode = providers.gradleProperty("jellypilot.versionCode").orElse("1").get().toInt()
    versionName = "2.2.1-android-bringup"
    ndk { abiFilters += "arm64-v8a" }
    testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
  }
  buildFeatures { compose = true; buildConfig = true }
  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }
  packaging {
    jniLibs { useLegacyPackaging = false; keepDebugSymbols += "**/*.so" }
    resources { merges += setOf("META-INF/AL2.0", "META-INF/LGPL2.1") }
  }
  lint { abortOnError = true; checkReleaseBuilds = true }
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
  implementation(project(":core-bridge"))
  implementation(project(":player-mpv"))
  implementation(platform(libs.compose.bom))
  implementation(libs.compose.foundation)
  implementation(libs.compose.material3)
  implementation(libs.compose.ui)
  implementation(libs.compose.ui.tooling.preview)
  implementation(libs.activity.compose)
  implementation(libs.lifecycle.runtime.compose)
  implementation(libs.lifecycle.viewmodel.compose)
  implementation(libs.core.ktx)
  implementation(libs.coroutines.android)
  implementation(libs.media3.session)
  implementation(libs.coil.compose)
  implementation(libs.coil.network.okhttp)
  debugImplementation(libs.compose.ui.tooling)
  androidTestImplementation(libs.junit)
  androidTestImplementation(libs.androidx.test.runner)
  androidTestImplementation(libs.androidx.test.core)
  androidTestImplementation(libs.androidx.test.ext.junit)
}
