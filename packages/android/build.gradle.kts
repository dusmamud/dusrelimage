// Root build file for the dusrelimage Android library.
// Versions are pinned here; CI uses Gradle 8.11.1 (see
// .github/workflows/mobile.yml) with JDK 17.
plugins {
    id("com.android.library") version "8.5.2" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
}
