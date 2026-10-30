package com.bluedoor.desktop

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.activity.OnBackPressedCallback

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    BlueDoorAndroid.init(applicationContext)
    BlueDoorAndroid.activity = java.lang.ref.WeakReference(this)
    java.io.File(cacheDir, "imports").deleteRecursively()
    java.io.File(cacheDir, "vault-transfer").deleteRecursively()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() { moveTaskToBack(true) }
    })
  }
  private external fun nativeBackground()
  private external fun nativeForeground()
  private external fun nativeStopped()
  override fun onStop() {
    if (!isChangingConfigurations) nativeStopped()
    super.onStop()
  }
  override fun onResume() {
    super.onResume()
    nativeForeground()
    BlueDoorAndroid.applyScreenAwake()
  }
  override fun onPause() {
    nativeBackground()
    super.onPause()
  }
}
