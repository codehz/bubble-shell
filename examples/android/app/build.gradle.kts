import java.io.File

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val repoRoot: File = rootProject.projectDir.parentFile.parentFile

android {
    namespace = "dev.bubble.shell"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.bubble.shell"
        minSdk = 24
        targetSdk = 35
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    sourceSets.getByName("main").assets.srcDir(layout.buildDirectory.dir("generated/bubbleAssets"))
}

base {
    archivesName.set("BubbleShell")
}

val fixtures = repoRoot.resolve("examples/web/fixtures")

tasks.register<Exec>("bundleFixtures") {
    workingDir = repoRoot
    inputs.dir(repoRoot.resolve("examples/app"))
    outputs.dir(fixtures)
    commandLine(
        "cargo", "run", "-q", "-p", "bubble-cli", "--", "bundle",
        "examples/app/fixtures/counter.tsx",
        "examples/app/fixtures/editing.tsx",
        "examples/app/fixtures/focus.tsx",
        "examples/app/fixtures/media.tsx",
        "examples/app/gestures.js",
        "--out", fixtures.absolutePath,
    )
}

tasks.register<Copy>("copyRuntimeJs") {
    from(repoRoot.resolve("crates/native/js/runtime.js"))
    into(repoRoot.resolve("examples/web"))
    rename { "runtime.js" }
    doLast {
        val bytes = repoRoot.resolve("crates/native/js/runtime.js").readBytes()
        val sdk = repoRoot.resolve("sdk/web/runtime.js")
        sdk.parentFile.mkdirs()
        sdk.writeBytes(bytes)
    }
}

tasks.register<Copy>("bubbleAssets") {
    dependsOn("bundleFixtures", "copyRuntimeJs")
    from(fixtures) { into("fixtures") }
    val font = File("/System/Library/Fonts/Supplemental/Arial.ttf")
    if (font.isFile) {
        from(font) { rename { "HostFont.ttf" } }
    }
    into(layout.buildDirectory.dir("generated/bubbleAssets"))
}

tasks.named("preBuild").configure { dependsOn("bubbleAssets") }

dependencies {
    implementation(project(":bubble"))
}
