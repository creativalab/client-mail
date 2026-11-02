# JNA and the UniFFI bindings are reached by reflection and from native code:
# keep them as they are when the app is minified.
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
-keep class com.ferrowl.core.** { *; }
-dontwarn java.awt.**
