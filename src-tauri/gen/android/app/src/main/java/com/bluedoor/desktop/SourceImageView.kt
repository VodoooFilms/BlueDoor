package com.bluedoor.desktop

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Matrix
import android.graphics.RectF
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.widget.ImageView

/** Bounded source bitmap with pinch zoom, drag and double-tap reset. */
class SourceImageView(context: Context) : ImageView(context) {
  private val transform = Matrix()
  private var zoom = 1f
  private var lastX = 0f
  private var lastY = 0f
  private val pinch = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
    override fun onScale(detector: ScaleGestureDetector): Boolean {
      val next = (zoom * detector.scaleFactor).coerceIn(1f, 6f)
      transform.postScale(next / zoom, next / zoom, detector.focusX, detector.focusY)
      zoom = next
      constrain()
      return true
    }
  })
  private val taps = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
    override fun onDown(event: MotionEvent) = true
    override fun onSingleTapUp(event: MotionEvent): Boolean { performClick(); return true }
    override fun onDoubleTap(event: MotionEvent): Boolean {
      if (zoom > 1f) fit() else {
        transform.postScale(2.5f, 2.5f, event.x, event.y)
        zoom = 2.5f
        constrain()
      }
      return true
    }
  })
  init {
    scaleType = ScaleType.MATRIX
    contentDescription = "Fuente original. Pellizca para ampliar, arrastra para desplazar y toca dos veces para ajustar."
  }
  override fun setImageBitmap(bitmap: Bitmap?) { super.setImageBitmap(bitmap); fit() }
  override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) { super.onSizeChanged(w, h, oldw, oldh); fit() }
  private fun fit() {
    val source = drawable ?: return
    if (width <= 0 || height <= 0 || source.intrinsicWidth <= 0 || source.intrinsicHeight <= 0) return
    transform.setRectToRect(RectF(0f, 0f, source.intrinsicWidth.toFloat(), source.intrinsicHeight.toFloat()), RectF(0f, 0f, width.toFloat(), height.toFloat()), Matrix.ScaleToFit.CENTER)
    zoom = 1f
    imageMatrix = transform
  }
  private fun constrain() {
    val source = drawable ?: return
    val bounds = RectF(0f, 0f, source.intrinsicWidth.toFloat(), source.intrinsicHeight.toFloat())
    transform.mapRect(bounds)
    val dx = if (bounds.width() <= width) (width - bounds.width()) / 2 - bounds.left else if (bounds.left > 0) -bounds.left else if (bounds.right < width) width - bounds.right else 0f
    val dy = if (bounds.height() <= height) (height - bounds.height()) / 2 - bounds.top else if (bounds.top > 0) -bounds.top else if (bounds.bottom < height) height - bounds.bottom else 0f
    transform.postTranslate(dx, dy)
    imageMatrix = transform
  }
  override fun onTouchEvent(event: MotionEvent): Boolean {
    pinch.onTouchEvent(event)
    taps.onTouchEvent(event)
    when (event.actionMasked) {
      MotionEvent.ACTION_DOWN -> { lastX = event.x; lastY = event.y }
      MotionEvent.ACTION_MOVE -> {
        if (!pinch.isInProgress && event.pointerCount == 1 && zoom > 1f) {
          transform.postTranslate(event.x - lastX, event.y - lastY)
          constrain()
        }
        lastX = event.x; lastY = event.y
      }
      MotionEvent.ACTION_POINTER_UP -> { val index = if (event.actionIndex == 0) 1 else 0; if (index < event.pointerCount) { lastX = event.getX(index); lastY = event.getY(index) } }
    }
    return true
  }
  override fun performClick(): Boolean { super.performClick(); return true }
}
