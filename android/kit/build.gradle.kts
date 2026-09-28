plugins {
    kotlin("jvm")
    kotlin("plugin.serialization")
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}
kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
    api("com.squareup.okhttp3:okhttp:5.3.2")
    api("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
    testImplementation(kotlin("test"))
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.11.0")
    testImplementation("com.squareup.okhttp3:mockwebserver3:5.3.2")
}

tasks.test {
    useJUnitPlatform()
    // End-to-end test against a running server: ALTIM_SERVER, ALTIM_USER, ALTIM_PASSWORD.
    listOf("ALTIM_SERVER", "ALTIM_USER", "ALTIM_PASSWORD").forEach { k -> System.getenv(k)?.let { environment(k, it) } }
    testLogging { events("passed", "skipped", "failed"); exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL }
}
