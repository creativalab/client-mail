// The Rust engine (ferrowl-core through ferrowl-ffi) packaged as an Android
// library: native .so files for each ABI plus the Kotlin bindings that
// UniFFI generates. Both are build outputs of ../ffi/build-android.sh and are
// not committed; the task below runs it when the Rust sources change.
plugins {
    alias(libs.plugins.android.library)
}

android {
    namespace = "com.ferrowl.core"
    // Latest AndroidX libraries need the Android 37 SDK platform to compile.
    compileSdk = 37

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }
    // src/main/jniLibs and src/main/kotlin are the default source folders,
    // so the generated files are picked up without extra configuration.
}

dependencies {
    // UniFFI's Kotlin code calls the native library through JNA.
    implementation("${libs.jna.get()}@aar")
    implementation(libs.kotlinx.coroutines.android)
}

val repoRoot = rootProject.projectDir.parentFile

// Builds the engine for arm64-v8a, armeabi-v7a and x86_64 and regenerates
// the bindings. Needs Rust (rustup), cargo-ndk and the NDK: see android/README.md.
val buildRustEngine by tasks.registering(Exec::class) {
    group = "build"
    description = "Builds the Rust engine and its Kotlin bindings"
    workingDir = repoRoot
    // A login shell, so Android Studio finds cargo even when it was started
    // from the desktop without ~/.cargo/bin in PATH.
    commandLine("bash", "-lc", "source \"\$HOME/.cargo/env\" 2>/dev/null; ffi/build-android.sh release android/core/src/main")
    inputs.dir(repoRoot.resolve("core/src"))
    inputs.dir(repoRoot.resolve("core/locales"))
    inputs.dir(repoRoot.resolve("ffi/src"))
    inputs.file(repoRoot.resolve("Cargo.lock"))
    outputs.dir(layout.projectDirectory.dir("src/main/jniLibs"))
    outputs.dir(layout.projectDirectory.dir("src/main/kotlin"))
}

tasks.named("preBuild") { dependsOn(buildRustEngine) }
