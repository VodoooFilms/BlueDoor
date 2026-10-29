package com.bluedoor.desktop

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Matrix
import android.os.SystemClock
import android.view.MotionEvent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SourceImageTest {
  @Test fun allExifOrientationsPreservePixelsAndReleaseReplacedBitmap() {
    val expected = listOf(Color.RED, Color.GREEN, Color.CYAN, Color.MAGENTA, Color.RED, Color.MAGENTA, Color.CYAN, Color.GREEN)
    for (orientation in 1..8) {
      val source = Bitmap.createBitmap(2, 3, Bitmap.Config.ARGB_8888)
      source.setPixels(intArrayOf(Color.RED, Color.GREEN, Color.BLUE, Color.YELLOW, Color.MAGENTA, Color.CYAN), 0, 2, 0, 0, 2, 3)
      val image = BlueDoorAndroid.orientBitmap(source, orientation)
      assertEquals("orientation $orientation width", if (orientation >= 5) 3 else 2, image.width)
      assertEquals("orientation $orientation height", if (orientation >= 5) 2 else 3, image.height)
      assertEquals("orientation $orientation top-left", expected[orientation - 1], image.getPixel(0, 0))
      assertEquals(orientation != 1, source.isRecycled)
      image.recycle()
    }
  }

  @Test fun doubleTapZoomsAndReturnsToFitWithoutAllocatingAnotherBitmap() {
    InstrumentationRegistry.getInstrumentation().runOnMainSync {
      val view = SourceImageView(InstrumentationRegistry.getInstrumentation().targetContext)
      val image = Bitmap.createBitmap(200, 300, Bitmap.Config.ARGB_8888)
      view.layout(0, 0, 400, 600)
      view.setImageBitmap(image)
      fun scale(): Float { val values = FloatArray(9); view.imageMatrix.getValues(values); return values[Matrix.MSCALE_X] }
      assertEquals(2f, scale(), .01f)
      val start = SystemClock.uptimeMillis()
      fun tap(offset: Long) {
        for ((action, delta) in listOf(MotionEvent.ACTION_DOWN to 0L, MotionEvent.ACTION_UP to 40L)) {
          val event = MotionEvent.obtain(start + offset, start + offset + delta, action, 200f, 300f, 0)
          view.onTouchEvent(event); event.recycle()
        }
      }
      tap(0); tap(100)
      assertEquals(5f, scale(), .01f)
      tap(1000); tap(1100)
      assertEquals(2f, scale(), .01f)
      assertFalse(image.isRecycled)
      view.setImageDrawable(null); image.recycle()
    }
  }
}
