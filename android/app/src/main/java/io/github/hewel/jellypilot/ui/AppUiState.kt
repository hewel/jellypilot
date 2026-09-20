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
  /** A Favorite/Played write for this item is in flight; action controls stay disabled until it settles. */
  val updating: Boolean = false,
)
internal data class LibraryUi(val id: String, val title: String)
internal data class ProfileUi(val key: String, val name: String, val server: String, val provider: String, val active: Boolean)

/** Lifecycle of one SDK browse session, mirrored from `BrowseStatus`. */
internal enum class BrowseUiStatus { Inactive, Loading, Empty, Ready, Failed }

/**
 * Compose projection of one `BrowseSession` snapshot. `slots` are the
 * SDK-bounded display window: `slots[i]` renders absolute index
 * `visibleStart + i`, and a null slot is a page the SDK has not delivered yet.
 * Item membership stays authoritative in Rust; this list is only a view.
 */
internal data class BrowserUi(
  val revision: ULong = 0uL,
  /** Process-unique generation of the owning session; keys viewport state so a replaced session never inherits another query's scroll position. */
  val generation: Long = 0L,
  val status: BrowseUiStatus = BrowseUiStatus.Inactive,
  val slots: List<MediaUi?> = emptyList(),
  val visibleStart: UInt = 0u,
  val totalCount: UInt = 0u,
  val isVirtual: Boolean = false,
  val loadingMore: Boolean = false,
  val error: String? = null,
  val retryable: Boolean = false,
  val retryBusy: Boolean = false,
  val refreshing: Boolean = false,
  val refreshError: String? = null,
) {
  /** Exclusive end of the absolute display indexes covered by [slots]. */
  val loadedEnd: UInt get() = visibleStart + slots.size.toUInt()
  val hasContent: Boolean get() = slots.any { it != null }
}

/** A projection for Compose; operation tokens, active scopes and user-data decisions stay in Rust. */
internal data class AppUiState(
  val destination: Destination = Destination.Home,
  val profiles: List<ProfileUi> = emptyList(),
  val activeName: String? = null,
  val libraries: List<LibraryUi> = emptyList(),
  val libraryId: String? = null,
  val items: List<MediaUi> = emptyList(),
  val browser: BrowserUi = BrowserUi(),
  val detail: MediaUi? = null,
  val detailItems: List<MediaUi> = emptyList(),
  val busy: Boolean = false,
  val loginBusy: Boolean = false,
  val quickConnectCode: String? = null,
  val error: String? = null,
  val showSignIn: Boolean = false,
  val showPlayer: Boolean = false,
  /** Raw search field text; mirrors the retained SDK query so a recreated field never shows stale or empty input. */
  val searchQuery: String = "",
  /** SDK sign-out deleted the saved credentials but teardown failed; the session stays connected for a cleanup retry and new playback/writes stay blocked. */
  val signOutCleanupPending: Boolean = false,
)
