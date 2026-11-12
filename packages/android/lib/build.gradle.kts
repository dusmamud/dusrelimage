plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.dusrel.image"
    compileSdk = 34

    defaultConfig {
        minSdk = 24
        // No targetSdk on a library; the consuming app sets it.
        // No versionCode/versionName either — the Rust core is the
        // version of record (0.1.0, see DusrelImage.version()).
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    sourceSets {
        // The JNI .so files built by scripts/build-jni.sh land here
        // (one dir per ABI). They are built in CI, never committed.
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    testOptions {
        unitTests.all {
            // JVM unit tests run against the HOST cdylib, not the
            // Android .so: the generated JNA code loads whatever
            // `uniffi.component.dusrelimage.libraryOverride` points at.
            // CI builds it first (`cargo build -p dusrelimage-uniffi
            // --release`) and passes its path via
            // DUSRELIMAGE_UNIFFI_LIB; locally, the default below is
            // <repo>/target/release/libdusrelimage_uniffi.so (Linux)
            // — on macOS set DUSRELIMAGE_UNIFFI_LIB to the .dylib.
            val hostLib = System.getenv("DUSRELIMAGE_UNIFFI_LIB")
                ?: rootDir.resolve("../../target/release/libdusrelimage_uniffi.so").canonicalPath
            it.systemProperty("uniffi.component.dusrelimage.libraryOverride", hostLib)
        }
    }
}

dependencies {
    // The generated UniFFI Kotlin code (dusrelimage.kt) uses JNA to
    // call the Rust cdylib — JNA is a runtime dependency of the lib.
    implementation("net.java.dev.jna:jna:5.14.0")

    testImplementation("junit:junit:4.13.2")
    // Minimal JSON parser for shared/test-vectors.json in the tests.
    testImplementation("org.json:json:20240303")
}
