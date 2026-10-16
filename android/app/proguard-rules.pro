# The Rust certificate verifier calls these classes through JNI.
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
# The core bindings call the Rust library through JNA, which finds methods and fields by name.
-keep class uniffi.** { *; }
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class * extends com.sun.jna.** { *; }
-dontwarn java.awt.**
