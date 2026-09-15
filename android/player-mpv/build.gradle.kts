plugins {
  alias(libs.plugins.android.library)
}

// libmpv and its dependencies are staged by `bun run task android mpv` at
// <repo>/target/android/mpv/arm64-v8a/{include,lib}. The Gradle build fails
// fast when they are absent rather than silently producing a broken APK.
val mpvStaging = rootProject.file("../target/android/mpv/arm64-v8a")

abstract class StageMpvLibraries : DefaultTask() {
  @get:InputDirectory abstract val sourceDirectory: DirectoryProperty
  @get:OutputDirectory abstract val outputDirectory: DirectoryProperty
  @get:javax.inject.Inject abstract val fileSystem: FileSystemOperations

  @TaskAction
  fun stage() {
    val source = sourceDirectory.get().asFile
    require(source.resolve("libmpv.so").isFile) {
      "libmpv.so missing at ${source.absolutePath}; run `bun run task android mpv` first"
    }
    fileSystem.sync {
      from(source) { into("arm64-v8a"); include("*.so") }
      into(outputDirectory)
    }
  }
}

val stageMpvJniLibs = tasks.register<StageMpvLibraries>("stageMpvJniLibs") {
  sourceDirectory.set(mpvStaging.resolve("lib"))
  outputDirectory.set(layout.buildDirectory.dir("stagedJniLibs"))
}

androidComponents.onVariants { variant ->
  variant.sources.jniLibs?.addGeneratedSourceDirectory(stageMpvJniLibs, StageMpvLibraries::outputDirectory)
}

android {
  namespace = "io.github.hewel.jellypilot.player"
  compileSdk = 37
  ndkVersion = "28.2.13676358"
  defaultConfig {
    minSdk = 26
    ndk { abiFilters += "arm64-v8a" }
    externalNativeBuild {
      cmake {
        arguments += "-DMPV_STAGING=${mpvStaging.absolutePath}"
        targets += "jellypilot_player"
      }
    }
  }
  externalNativeBuild {
    cmake { path = file("src/main/cpp/CMakeLists.txt"); version = "3.22.1" }
  }
  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }
  packaging {
    jniLibs { useLegacyPackaging = false }
  }
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
  implementation(libs.media3.common)
  testImplementation(libs.junit)
}
