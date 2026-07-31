plugins {
  id("com.android.application")
  id("org.jetbrains.kotlin.android")
}

val repositoryRoot = rootDir.resolve("../..").canonicalFile
val spikeInput = repositoryRoot.resolve("target/android-prover-spike/app-input")
val spikeArtifacts = repositoryRoot.resolve("target/android-prover-spike/artifacts")

android {
  namespace = "network.midnight.proverspike"
  compileSdk = 36
  ndkVersion = "27.1.12297006"

  defaultConfig {
    applicationId = "network.midnight.proverspike"
    minSdk = 24
    targetSdk = 36
    versionCode = 1
    versionName = "0.1"
    ndk {
      abiFilters += "arm64-v8a"
    }
  }

  buildTypes {
    release {
      isDebuggable = true
      isMinifyEnabled = false
      signingConfig = signingConfigs.getByName("debug")
    }
  }

  sourceSets {
    getByName("main") {
      java.srcDir(spikeInput.resolve("generated/kotlin"))
      jniLibs.srcDir(spikeInput.resolve("jniLibs"))
      assets.srcDir(spikeArtifacts)
    }
  }

  androidResources {
    noCompress += listOf("", "bls_midnight_2p15", "prover", "verifier", "bzkir", "bin")
  }

  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }
}

kotlin {
  jvmToolchain(17)
}

dependencies {
  implementation("net.java.dev.jna:jna:5.17.0@aar")
}
