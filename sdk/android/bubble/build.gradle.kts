import java.io.File

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

val repoRoot: File = rootProject.projectDir.parentFile.parentFile

fun newestChild(dir: File): File {
    val children = dir.listFiles()?.filter { it.isDirectory }.orEmpty()
    if (children.isEmpty()) error("no directories in $dir")
    fun parts(name: String) = name.split('.').map { it.toIntOrNull() ?: 0 }
    return children.maxWith(compareBy({ parts(it.name).getOrElse(0) { 0 } }, { parts(it.name).getOrElse(1) { 0 } }, { parts(it.name).getOrElse(2) { 0 } }))
}

fun ndkDir(): File {
    System.getenv("ANDROID_NDK_HOME")?.let { return File(it) }
    val sdk = File(System.getenv("ANDROID_HOME") ?: error("ANDROID_HOME is unset"))
    return newestChild(sdk.resolve("ndk"))
}

fun androidClang(ndk: File): Triple<File, File, File> {
    val os = System.getProperty("os.name").lowercase()
    val arch = System.getProperty("os.arch")
    val (host, alt) = when {
        os.contains("mac") && arch == "aarch64" -> "darwin-arm64" to "darwin-x86_64"
        os.contains("mac") -> "darwin-x86_64" to "darwin-arm64"
        os.contains("linux") && (arch == "aarch64" || arch == "arm64") -> "linux-aarch64" to "linux-x86_64"
        os.contains("linux") -> "linux-x86_64" to "linux-aarch64"
        else -> error("unsupported NDK host $os-$arch")
    }
    val prebuilt = ndk.resolve("toolchains/llvm/prebuilt")
    val pre = listOf(host, alt).map { prebuilt.resolve(it) }.firstOrNull { it.isDirectory }
        ?: error("Android NDK prebuilt missing under $prebuilt")
    val clang = pre.resolve("bin").listFiles()
        ?.filter { it.name.matches(Regex("aarch64-linux-android\\d+-clang")) }
        ?.maxByOrNull { Regex("\\d+").find(it.name)?.value?.toInt() ?: 0 }
        ?: error("aarch64 Android clang not found in ${pre.resolve("bin")}")
    return Triple(clang, pre.resolve("bin/llvm-ar"), pre.resolve("sysroot"))
}

android {
    namespace = "dev.bubble.shell"
    compileSdk = 35
    ndkVersion = ndkDir().name

    defaultConfig {
        minSdk = 24
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    sourceSets.getByName("main").jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))
}

tasks.register("cargoAndroid") {
    val so = repoRoot.resolve("target/aarch64-linux-android/release/libbubble_ffi.so")
    inputs.dir(repoRoot.resolve("crates"))
    outputs.file(so)
    doLast {
        val (cc, ar, sysroot) = androidClang(android.ndkDirectory)
        exec {
            workingDir = repoRoot
            environment("CC_aarch64_linux_android", cc.absolutePath)
            environment("AR_aarch64_linux_android", ar.absolutePath)
            environment("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", cc.absolutePath)
            environment("BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android", "--sysroot=${sysroot.absolutePath}")
            commandLine("cargo", "build", "-p", "bubble-ffi", "--release", "--target", "aarch64-linux-android")
        }
        val dest = layout.buildDirectory.dir("rustJniLibs/arm64-v8a").get().asFile.resolve("libbubble_ffi.so")
        dest.parentFile.mkdirs()
        so.copyTo(dest, overwrite = true)
    }
}

tasks.named("preBuild").configure { dependsOn("cargoAndroid") }
