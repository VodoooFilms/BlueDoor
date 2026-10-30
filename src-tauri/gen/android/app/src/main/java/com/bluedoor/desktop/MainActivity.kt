package com.bluedoor.desktop

import android.os.Bundle
import android.view.WindowManager
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.activity.OnBackPressedCallback

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    BlueDoorAndroid.init(applicationContext)
    BlueDoorAndroid.activity = java.lang.ref.WeakReference(this)
    java.io.File(cacheDir, "imports").deleteRecursively()
    java.io.File(cacheDir, "vault-transfer").deleteRecursively()
    super.onCreate(savedInstanceState)
    // Keep the complete WebView (including fixed dialogs) inside system bars
    // and display cutouts; recomputed automatically when orientation changes.
    val content = findViewById<android.view.View>(android.R.id.content)
    val blue = androidx.core.content.ContextCompat.getColor(this, R.color.bluedoor_blue)
    window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(blue))
    content.setBackgroundColor(blue)
    window.statusBarColor = android.graphics.Color.TRANSPARENT
    window.navigationBarColor = android.graphics.Color.TRANSPARENT
    if (android.os.Build.VERSION.SDK_INT >= 29) {
      window.isStatusBarContrastEnforced = false
      window.isNavigationBarContrastEnforced = false
    }
    androidx.core.view.WindowCompat.getInsetsController(window, content).apply {
      isAppearanceLightStatusBars = false
      isAppearanceLightNavigationBars = false
    }
    androidx.core.view.WindowCompat.setDecorFitsSystemWindows(window, false)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val safe = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      view.setPadding(safe.left, safe.top, safe.right, safe.bottom)
      insets
    }
    ViewCompat.requestApplyInsets(content)
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
