# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
-keep class com.bluedoor.desktop.AndroidBridge { *; }
-keepclassmembers class com.bluedoor.desktop.MainActivity { native <methods>; }
# ONNX's JNI resolves tensor/value classes and enum members by their original
# names. Keeping only native methods still lets R8 rename those support types.
-keep class ai.onnxruntime.** { *; }

-keep class org.vosk.** { *; }
-keep class com.sun.jna.** { *; }
# JNA ships optional desktop AWT hooks; Android uses only native library calls.
-dontwarn java.awt.Component
-dontwarn java.awt.GraphicsEnvironment
-dontwarn java.awt.HeadlessException
-dontwarn java.awt.Window
