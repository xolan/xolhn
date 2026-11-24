plugins {
    id("com.android.application")
}

android {
    namespace = "dev.xolhn.app"
    compileSdk = 34

    val workspaceVersion = rootProject.file("../Cargo.toml")
        .takeIf { it.exists() }
        ?.useLines { lines ->
            lines.mapNotNull { line ->
                Regex("""^\s*version\s*=\s*"([^"]+)"""").find(line)?.groupValues?.get(1)
            }.firstOrNull()
        }
    val appVersionName = providers.gradleProperty("xolhnVersion")
        .orNull
        ?.trim()
        ?.takeIf { it.isNotEmpty() }
        ?: workspaceVersion
        ?: "1.0.1"
    val semverParts = appVersionName
        .substringBefore("-")
        .substringBefore("+")
        .split(".")
    val appVersionCode = (semverParts.getOrNull(0)?.toIntOrNull() ?: 1) * 10000 +
        (semverParts.getOrNull(1)?.toIntOrNull() ?: 0) * 100 +
        (semverParts.getOrNull(2)?.toIntOrNull() ?: 0)

    val releaseStorePath = System.getenv("ANDROID_KEYSTORE_PATH")?.takeIf { it.isNotBlank() }
    val releaseStorePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")?.takeIf { it.isNotBlank() }
    val releaseKeyAlias = System.getenv("ANDROID_KEY_ALIAS")?.takeIf { it.isNotBlank() }
    val releaseKeyPassword = System.getenv("ANDROID_KEY_PASSWORD")?.takeIf { it.isNotBlank() }
    val hasReleaseSigning =
        releaseStorePath != null &&
            releaseStorePassword != null &&
            releaseKeyAlias != null &&
            releaseKeyPassword != null &&
            file(releaseStorePath).exists()

    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = file(releaseStorePath!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    defaultConfig {
        applicationId = "dev.xolhn.app"
        minSdk = 26
        targetSdk = 34
        versionCode = appVersionCode
        versionName = appVersionName

        val abiList = providers.gradleProperty("xolhnAbi")
            .orNull
            ?.split(",")
            ?.map { it.trim() }
            ?.filter { it.isNotEmpty() }
            ?.takeIf { it.isNotEmpty() }
            ?: listOf("arm64-v8a")

        ndk {
            abiFilters += abiList
        }

        manifestPlaceholders["nativeLibraryName"] = "xolhn_android"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (hasReleaseSigning) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
        debug {
            isDebuggable = true
            isJniDebuggable = true
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    packaging {
        jniLibs {
            keepDebugSymbols += listOf(
                "*/arm64-v8a/libxolhn_android.so",
                "*/x86_64/libxolhn_android.so",
            )
        }
    }
}

dependencies {
    implementation("androidx.core:core:1.12.0")
    implementation("androidx.core:core-splashscreen:1.0.1")
}
