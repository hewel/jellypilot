package io.github.hewel.jellypilot

import android.app.LocaleManager
import android.content.Context
import android.content.SharedPreferences
import android.content.res.Configuration
import android.os.Build
import androidx.annotation.MainThread
import androidx.annotation.StringRes
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import io.github.hewel.jellypilot.ui.LanguagePreference
import io.github.hewel.jellypilot.ui.PreferencesUi
import io.github.hewel.jellypilot.ui.ThemePreference
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** Platform presentation state only; business preferences and credentials remain SDK-owned. */
internal class AndroidPreferences(
  context: Context,
  private val storage: SharedPreferences = context.getSharedPreferences("presentation", Context.MODE_PRIVATE),
) {
  private val application = context.applicationContext
  private val writes = Mutex()

  fun presentation() = PreferencesUi(
    theme = ThemePreference.entries.firstOrNull { it.name == storage.getString("theme", null) } ?: ThemePreference.System,
    language = language(),
    reducedMotion = storage.getBoolean("reduced_motion", false),
  )

  fun language(): LanguagePreference = when (application.applicationLocales()[0]?.language) {
    "en" -> LanguagePreference.English
    "zh" -> LanguagePreference.Chinese
    else -> LanguagePreference.System
  }

  suspend fun setTheme(value: ThemePreference) = persist { putString("theme", value.name) }

  suspend fun setReducedMotion(value: Boolean) = persist { putBoolean("reduced_motion", value) }

  @MainThread
  fun setLanguage(value: LanguagePreference) {
    // AppCompat owns storage before API 33 and delegates to the framework thereafter.
    // A second preference key would drift when language changes through system Settings.
    AppCompatDelegate.setApplicationLocales(LocaleListCompat.forLanguageTags(when (value) {
      LanguagePreference.System -> ""
      LanguagePreference.English -> "en"
      LanguagePreference.Chinese -> "zh-Hans"
    }))
  }

  private suspend fun persist(edit: SharedPreferences.Editor.() -> Unit) = writes.withLock {
    withContext(Dispatchers.IO) {
      if (!storage.edit().apply(edit).commit()) throw IOException("Could not save presentation preferences")
    }
  }
}

private fun Context.applicationLocales(): LocaleListCompat = if (Build.VERSION.SDK_INT >= 33) {
  LocaleListCompat.wrap(getSystemService(LocaleManager::class.java).applicationLocales)
} else {
  AppCompatDelegate.getApplicationLocales()
}

/** AppCompat wraps Activity resources on older Android; long-lived app owners need the same locale. */
internal fun Context.localizedString(@StringRes resource: Int, vararg arguments: Any): String {
  val locales = applicationLocales()
  val localized = if (locales.isEmpty) this else createConfigurationContext(Configuration(resources.configuration).apply {
    setLocales(android.os.LocaleList.forLanguageTags(locales.toLanguageTags()))
  })
  return if (arguments.isEmpty()) localized.getString(resource) else localized.getString(resource, *arguments)
}
