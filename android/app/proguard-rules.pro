# JNA finds classes and fields by name, and the UniFFI bindings' structures
# and callbacks are reached through it.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class com.danifunker.misterkvms.core.** { *; }
-dontwarn java.awt.**
# The core's JNI functions (core/src/android.rs).
-keep class com.danifunker.misterkvms.AndroidCore { native <methods>; }
