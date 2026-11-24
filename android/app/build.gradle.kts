plugins {
    id("com.android.application")
}

android {
    namespace = "dev.xolhn.app"
    compileSdk = 34

    defaultConfig {
        applicationId = "dev.xolhn.app"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "1.0.0"

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
