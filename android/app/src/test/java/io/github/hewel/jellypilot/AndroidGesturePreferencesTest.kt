package io.github.hewel.jellypilot

import android.app.Application
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import io.github.hewel.jellypilot.ui.ThemePreference
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidGesturePreferencesTest {
  @Test fun gestureChoiceSurvivesNewPreferencesOwnerWithoutChangingOtherPresentationSettings() = runBlocking {
    val context = ApplicationProvider.getApplicationContext<Application>()
    val name = "gesture-preference-${UUID.randomUUID()}"
    val storage = context.getSharedPreferences(name, Context.MODE_PRIVATE)
    try {
      val initial = AndroidPreferences(context, storage)
      assertTrue(initial.presentation().playerGestures)
      initial.setTheme(ThemePreference.Dark)
      initial.setPlayerGestures(false)
      val reopened = AndroidPreferences(context, context.getSharedPreferences(name, Context.MODE_PRIVATE))
      assertFalse(reopened.presentation().playerGestures)
      assertEquals(ThemePreference.Dark, reopened.presentation().theme)
      reopened.setPlayerGestures(true)
      assertTrue(AndroidPreferences(context, storage).presentation().playerGestures)
    } finally { context.deleteSharedPreferences(name) }
  }
}
