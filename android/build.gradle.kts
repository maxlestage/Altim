// Altim Android: `kit` = pure Kotlin core (API models, private login, live prices, formats), tested on the JVM;
// `app` = Jetpack Compose app, client of the private Altim server (same as the iPhone app).
plugins {
    id("com.android.application") version "8.13.2" apply false
    kotlin("android") version "2.4.20" apply false
    kotlin("jvm") version "2.4.20" apply false
    kotlin("plugin.serialization") version "2.4.20" apply false
    kotlin("plugin.compose") version "2.4.20" apply false
}
