package io.github.hewel.jellypilot.ui

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.ffi.ProfileScopeRef

internal enum class Destination(@StringRes val title: Int, @DrawableRes val icon: Int) {
  Home(R.string.home, R.drawable.ic_home),
  Library(R.string.library, R.drawable.ic_grid),
  Search(R.string.search, R.drawable.ic_search),
  Account(R.string.account, R.drawable.ic_user),
}

internal data class ArtworkUi(val imageId: String, val scope: ProfileScopeRef) {
  val cacheKey: String get() = "${scope.profileKey}|$imageId|384"
}
internal data class MediaUi(
  val id: String,
  val title: String,
  val itemType: String,
  val metadata: String,
  val artwork: ArtworkUi?,
  val overview: String,
  val favorite: Boolean,
  val played: Boolean,
)
internal data class LibraryUi(val id: String, val title: String)
internal data class ProfileUi(val key: String, val name: String, val server: String, val provider: String, val active: Boolean)

/** A projection for Compose; operation tokens, active scopes and user-data decisions stay in Rust. */
internal data class AppUiState(
  val destination: Destination = Destination.Home,
  val profiles: List<ProfileUi> = emptyList(),
  val activeName: String? = null,
  val libraries: List<LibraryUi> = emptyList(),
  val libraryId: String? = null,
  val items: List<MediaUi> = emptyList(),
  val detail: MediaUi? = null,
  val detailItems: List<MediaUi> = emptyList(),
  val busy: Boolean = false,
  val loginBusy: Boolean = false,
  val quickConnectCode: String? = null,
  val error: String? = null,
  val hasMore: Boolean = false,
  val showSignIn: Boolean = false,
  val showPlayer: Boolean = false,
)
