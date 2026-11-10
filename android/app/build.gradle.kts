plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// Version comes from the release workflow (-PappVersion=1.2.3 -PappVersionCode=10203).
val appVersion = (project.findProperty("appVersion") as String?) ?: "0.2.0"
val appVersionCode = ((project.findProperty("appVersionCode") as String?) ?: "2").toInt()

android {
    namespace = "dev.zimaconnect"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.zimaconnect"
        minSdk = 26
        targetSdk = 35
        versionCode = appVersionCode
        versionName = appVersion
    }

    // The Rust library is built by cargo-ndk into src/main/jniLibs (see the release workflow).
    sourceSets["main"].jniLibs.srcDir("src/main/jniLibs")

    signingConfigs {
        create("release") {
            // CI supplies a keystore; locally the debug key is used so `assembleRelease` just works.
            val ks = System.getenv("ZC_KEYSTORE")
            if (ks != null && file(ks).exists()) {
                storeFile = file(ks)
                storePassword = System.getenv("ZC_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ZC_KEY_ALIAS")
                keyPassword = System.getenv("ZC_KEY_PASSWORD")
            } else {
                initWith(signingConfigs.getByName("debug"))
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("release")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
    }
    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material.icons)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.zxing.embedded)
    implementation(libs.zxing.core)
}
