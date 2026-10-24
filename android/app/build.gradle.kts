plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The API level Google Play requires of new apps and updates; Google raises
// it every August (docs/ANDROID.md).
val playTargetSdk = 36

// From scripts/build-android.sh: the release version (1.0.N) and a build
// number that only grows (the CI run number), as Play requires.
val appVersionName = providers.gradleProperty("versionName").getOrElse("1.0.0")
val appVersionCode = providers.gradleProperty("versionCode").getOrElse("1").toInt()

// The upload key (docs/ANDROID.md): a PKCS12 keystore, one password for the
// store and the key. Without it release builds are unsigned.
val keystore = providers.environmentVariable("ANDROID_KEYSTORE_PATH").orNull
val keystorePassword = providers.environmentVariable("ANDROID_KEYSTORE_PASSWORD").orNull
val keyAlias = providers.environmentVariable("ANDROID_KEY_ALIAS").orNull

android {
    namespace = "com.danifunker.misterkvms"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.danifunker.misterkvms"
        // Same as MIN_SDK in scripts/build-android.sh.
        minSdk = 29
        targetSdk = playTargetSdk
        versionCode = appVersionCode
        versionName = appVersionName
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    signingConfigs {
        if (keystore != null && keystorePassword != null && keyAlias != null) {
            create("upload") {
                storeFile = file(keystore)
                storePassword = keystorePassword
                this.keyAlias = keyAlias
                keyPassword = keystorePassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("upload")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        // The version in Settings ▸ About.
        buildConfig = true
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.13.0")
    // The UniFFI bindings call the core through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
