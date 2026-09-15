package io.github.hewel.jellypilot.ui

import android.content.res.AssetManager
import android.graphics.Typeface
import android.graphics.fonts.Font as PlatformFont
import android.graphics.fonts.FontFamily as PlatformFontFamily
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Native projection of jellypilot-ui's palette and the accepted Paper Mobile geometry.
private val DarkColors = darkColorScheme(
  primary = Color(0xFF6366F1), onPrimary = Color.White,
  primaryContainer = Color(0xFF1A1B37), onPrimaryContainer = Color(0xFFE0E2FF),
  secondary = Color(0xFF818CF8), onSecondary = Color(0xFF0B0A24),
  secondaryContainer = Color(0xFF18192F), onSecondaryContainer = Color(0xFF818CF8),
  tertiary = Color(0xFF34D399), onTertiary = Color(0xFF001F16),
  background = Color(0xFF0A0B0E), onBackground = Color.White,
  surface = Color(0xFF15161C), onSurface = Color.White,
  surfaceVariant = Color(0xFF1B1D26), onSurfaceVariant = Color(0xFFA1A1AA),
  surfaceContainerLowest = Color(0xFF0E0F14), surfaceContainerLow = Color(0xFF121318),
  surfaceContainer = Color(0xFF191A21), surfaceContainerHigh = Color(0xFF20222B),
  surfaceContainerHighest = Color(0xFF2A2D38),
  outline = Color(0xFF52525B), outlineVariant = Color(0xFF23262B),
  error = Color(0xFFFF6B7A), onError = Color(0xFF330006),
  errorContainer = Color(0xFF4B1119), onErrorContainer = Color(0xFFFFD9DE),
)
private val LightColors = lightColorScheme(
  primary = Color(0xFF6366F1), onPrimary = Color.White,
  primaryContainer = Color(0xFFE0E2FF), onPrimaryContainer = Color(0xFF1F2152),
  secondary = Color(0xFF4F46E5), onSecondary = Color.White,
  secondaryContainer = Color(0xFFE8E8F9), onSecondaryContainer = Color(0xFF4F46E5),
  tertiary = Color(0xFF047857), onTertiary = Color.White,
  background = Color(0xFFFBFCFD), onBackground = Color(0xFF0F172A),
  surface = Color.White, onSurface = Color(0xFF0F172A),
  surfaceVariant = Color(0xFFE5EAF1), onSurfaceVariant = Color(0xFF64748B),
  surfaceContainerLowest = Color(0xFFFAFAFA), surfaceContainerLow = Color(0xFFF1F5F9),
  surfaceContainer = Color(0xFFE9EDF2), surfaceContainerHigh = Color(0xFFE2E8F0),
  surfaceContainerHighest = Color(0xFFD3DAE4),
  outline = Color(0xFF94A3B8), outlineVariant = Color(0xFFE7ECF3),
  error = Color(0xFF4B1119), onError = Color(0xFFFFD9DE),
  errorContainer = Color(0xFFFFD9DE), onErrorContainer = Color(0xFF4B1119),
)

@Immutable
internal data class PilotColors(val body: Color, val metadata: Color, val sidebar: Color, val favorite: Color)
private val DarkPilot = PilotColors(Color(0xFFD4D4D8), Color(0xFFA1A1AA), Color(0xFF0F1016), Color(0xFFF87171))
private val LightPilot = PilotColors(Color(0xFF475569), Color(0xFF64748B), Color(0xFFFAFAFA), Color(0xFFE11D48))
internal val LocalPilotColors = staticCompositionLocalOf { LightPilot }

@Composable
internal fun PilotTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
  val assets = LocalContext.current.applicationContext.assets
  val chinese = LocalConfiguration.current.locales[0].language == "zh"
  val family = remember(assets, chinese) {
    if (Build.VERSION.SDK_INT >= 29) {
      FontFamily(brandTypeface(assets))
    } else {
      // Custom glyph fallback chains are public from API 29. Older devices use their CJK fallback.
      FontFamily(Typeface.createFromAsset(assets, if (chinese) "fonts/MiSansVF.ttf" else "fonts/ManropeV5VF.ttf"))
    }
  }
  fun style(size: Int, line: Int, weight: FontWeight = FontWeight.Normal) = TextStyle(
    fontFamily = family, fontSize = size.sp, lineHeight = line.sp, fontWeight = weight,
  )
  val typography = remember(family) {
    Typography(
      displayLarge = style(40, 48, FontWeight.Bold), displayMedium = style(36, 44, FontWeight.Bold),
      displaySmall = style(32, 40, FontWeight.Bold), headlineLarge = style(32, 40, FontWeight.Bold),
      headlineMedium = style(28, 36, FontWeight.Bold), headlineSmall = style(24, 32, FontWeight.Bold),
      titleLarge = style(20, 28, FontWeight.SemiBold), titleMedium = style(16, 24, FontWeight.SemiBold),
      titleSmall = style(14, 20, FontWeight.SemiBold), bodyLarge = style(16, 24),
      bodyMedium = style(14, 20), bodySmall = style(12, 18),
      labelLarge = style(14, 20, FontWeight.SemiBold), labelMedium = style(12, 16, FontWeight.Medium),
      labelSmall = style(10, 14, FontWeight.Medium),
    )
  }
  CompositionLocalProvider(LocalPilotColors provides if (dark) DarkPilot else LightPilot) {
    MaterialTheme(
      colorScheme = if (dark) DarkColors else LightColors,
      typography = typography,
      shapes = Shapes(
        extraSmall = RoundedCornerShape(6.dp), small = RoundedCornerShape(8.dp),
        medium = RoundedCornerShape(12.dp), large = RoundedCornerShape(20.dp),
        extraLarge = RoundedCornerShape(20.dp),
      ),
      content = content,
    )
  }
}

// Custom fallback typefaces are not covered by createFromAsset's platform cache.
// Keep their shared font buffers across Activity/composition recreation.
private var cachedBrandTypeface: Typeface? = null

@RequiresApi(29)
@Synchronized
private fun brandTypeface(assets: AssetManager): Typeface {
  cachedBrandTypeface?.let { return it }
  fun family(path: String): PlatformFontFamily {
    val regular = PlatformFont.Builder(assets, path)
      .setWeight(400).setFontVariationSettings("'wght' 400").build()
    val buffer = regular.buffer
    fun font(weight: Int) = PlatformFont.Builder(buffer)
      .setWeight(weight).setFontVariationSettings("'wght' $weight").build()
    return PlatformFontFamily.Builder(regular)
      .addFont(font(500)).addFont(font(600)).addFont(font(700)).build()
  }
  return Typeface.CustomFallbackBuilder(family("fonts/ManropeV5VF.ttf"))
    .addCustomFallback(family("fonts/MiSansVF.ttf"))
    .setSystemFallback("sans-serif").build().also { cachedBrandTypeface = it }
}
