pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // The Kotlin half of the Rust certificate verifier is published only here.
        exclusiveContent {
            forRepository {
                maven("https://raw.githubusercontent.com/rustls/rustls-platform-verifier/maven-archive/android-release-support/maven/")
            }
            filter { includeGroup("org.rustls") }
        }
    }
}

rootProject.name = "ozero-mail"
include(":app")
