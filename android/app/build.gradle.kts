plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

val generatedDir = layout.buildDirectory.dir("generated/ozero")

android {
    namespace = "com.ozeromail.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.ozeromail.app"
        minSdk = 33
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

}

kotlin {
    jvmToolchain(17)
}

// Rust core: .so per ABI, then Kotlin bindings read from the built library.
val repoDir = rootDir.parentFile
val coreSources = fileTree(repoDir.resolve("core")) { exclude("target") }

val buildCore by tasks.registering(Exec::class) {
    inputs.files(coreSources)
    outputs.dir(generatedDir.map { it.dir("jniLibs") })
    workingDir = repoDir.resolve("core")
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "--platform", android.defaultConfig.minSdk.toString(),
        "-o", generatedDir.get().dir("jniLibs").asFile.path,
        "build", "--release", "-p", "ozero-api",
    )
}

val generateBindings by tasks.registering(Exec::class) {
    dependsOn(buildCore)
    inputs.files(coreSources)
    outputs.dir(generatedDir.map { it.dir("kotlin") })
    workingDir = repoDir.resolve("core")
    commandLine(
        "cargo", "run", "-p", "ozero-bindgen", "--",
        "generate", "--language", "kotlin", "--no-format",
        "--library", generatedDir.get().file("jniLibs/arm64-v8a/libozero_api.so").asFile.path,
        "--out-dir", generatedDir.get().dir("kotlin").asFile.path,
    )
}

val generateStrings by tasks.registering(Exec::class) {
    inputs.dir(repoDir.resolve("i18n"))
    inputs.file(repoDir.resolve("tools/gen-strings.py"))
    outputs.dir(generatedDir.map { it.dir("res") })
    commandLine("python3", repoDir.resolve("tools/gen-strings.py").path, generatedDir.get().dir("res").asFile.path)
}

// Source dirs come from the task providers, so anything reading them depends on the tasks.
android.sourceSets["main"].apply {
    jniLibs.srcDir(buildCore.map { it.outputs.files.singleFile })
    java.srcDir(generateBindings.map { it.outputs.files.singleFile })
    res.srcDir(generateStrings.map { it.outputs.files.singleFile })
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.material3)
    implementation(libs.activity.compose)
    implementation(libs.jna) { artifact { type = "aar" } }
}
