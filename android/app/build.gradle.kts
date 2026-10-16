import javax.inject.Inject
import org.gradle.process.ExecOperations

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

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

    buildTypes {
        release {
            proguardFiles("proguard-rules.pro")
        }
    }
}

kotlin {
    jvmToolchain(17)
}

// Generated sources: Rust core .so per ABI, Kotlin bindings read from it, strings.xml from i18n/.
// Registered through the variant API so every task that reads them depends on their tasks.
abstract class RunTool : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations
    @get:InputFiles @get:PathSensitive(PathSensitivity.RELATIVE) abstract val sources: ConfigurableFileCollection
    @get:Input abstract val command: ListProperty<String>
    @get:Internal abstract val workDir: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun run() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        exec.exec {
            workingDir = workDir.get().asFile
            commandLine(command.get().map { it.replace("{out}", out.path) })
        }
    }
}

// AGP replaces these with its own generated dirs; the default lets a task run on its own.
tasks.withType<RunTool>().configureEach {
    outputDir.convention(layout.buildDirectory.dir("generated/ozero/$name"))
}

val repoDir = rootDir.parentFile
val coreDir = layout.projectDirectory.dir("../../core")

val buildCore by tasks.registering(RunTool::class) {
    sources.from(fileTree(coreDir) { exclude("target") })
    workDir = coreDir
    command.addAll(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "--platform", android.defaultConfig.minSdk.toString(),
        "-o", "{out}",
        "build", "--release", "-p", "ozero-api",
    )
}

val generateBindings by tasks.registering(RunTool::class) {
    val library = buildCore.flatMap { it.outputDir.file("arm64-v8a/libozero_api.so") }
    sources.from(library)
    workDir = coreDir
    command.addAll("cargo", "run", "-p", "ozero-bindgen", "--", "generate", "--language", "kotlin", "--no-format")
    command.add("--library")
    command.add(library.map { it.asFile.path })
    command.addAll("--out-dir", "{out}")
}

val generateStrings by tasks.registering(RunTool::class) {
    sources.from(repoDir.resolve("i18n"), repoDir.resolve("tools/gen-strings.py"))
    workDir = layout.projectDirectory
    command.addAll("python3", repoDir.resolve("tools/gen-strings.py").path, "{out}")
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(buildCore, RunTool::outputDir)
        variant.sources.java?.addGeneratedSourceDirectory(generateBindings, RunTool::outputDir)
        variant.sources.res?.addGeneratedSourceDirectory(generateStrings, RunTool::outputDir)
    }
}

// The verifier's Kotlin half must be the version the Rust crate in Cargo.lock expects.
val verifierVersion = providers.fileContents(coreDir.file("Cargo.lock")).asText.map { lock ->
    Regex("name = \"rustls-platform-verifier-android\"\nversion = \"([^\"]+)\"")
        .find(lock)?.groupValues?.get(1)
        ?: error("rustls-platform-verifier-android is missing from core/Cargo.lock")
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.material3)
    implementation(libs.activity.compose)
    implementation(libs.jna) { artifact { type = "aar" } }
    implementation(libs.coroutines.android)
    implementation(libs.work.runtime)
    implementation(verifierVersion.map { "org.rustls:rustls-platform-verifier:$it" })
}
