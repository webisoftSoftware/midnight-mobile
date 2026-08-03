# JNA maps these classes field-by-field onto the native ABI, instantiating them
# reflectively and reading @JvmField members in declaration order. R8 must not
# rename, remove, or reorder them in release builds.
-keep class dev.oneam.midnightmobile.localprover.ParameterDescriptor { *; }
-keep class dev.oneam.midnightmobile.localprover.CircuitDescriptor { *; }
-keep class dev.oneam.midnightmobile.localprover.NativeResponse { *; }
-keep class * extends com.sun.jna.Structure { *; }
-keep class com.sun.jna.** { *; }
