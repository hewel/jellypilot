plugins {
  alias(libs.plugins.android.library)
}

extensions.configure<com.android.build.api.dsl.LibraryExtension> {
  namespace = "io.github.hewel.jellypilot.bridge"
  compileSdk = 37
  ndkVersion = "28.2.13676358"
  defaultConfig {
    minSdk = 26
    ndk { abiFilters += "arm64-v8a" }
  }
  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }
  // `bun run task android rust` stages libjellypilot_ffi.so into
  // build/jniLibs/<abi>; `bun run task android bindings` generates Kotlin
  // into build/generated/uniffi. Both stay out of src/ as build artifacts.
  sourceSets["main"].apply {
    jniLibs.directories += "build/jniLibs"
    kotlin.directories += "build/generated/uniffi"
  }
  packaging {
    jniLibs { useLegacyPackaging = false }
  }
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
  // The default JAR does not package Android's libjnidispatch into the APK.
  implementation(variantOf(libs.jna) { artifactType("aar") })
  implementation(libs.coroutines.android)
  implementation(libs.androidx.annotation)
}
