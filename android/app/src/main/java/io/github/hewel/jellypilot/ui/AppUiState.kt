package io.github.hewel.jellypilot.ui

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.ffi.ProfileScopeRef

internal enum class Destination(@StringRes val title: Int, @DrawableRes val icon: Int) {
  Home(R.string.home, R.drawable.ic_home),
  Lists(R.string.personal_lists, R.drawable.ic_bookmark),
  Library(R.string.library, R.drawable.ic_grid),
  Search(R.string.search, R.drawable.ic_search),
  Account(R.string.account, R.drawable.ic_user),
}

internal data class ArtworkUi(val imageId: String, val scope: ProfileScopeRef, val requestedWidth: Int = 384) {
  val cacheKey: String get() = "${scope.profileKey}|$imageId|$requestedWidth"
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
  val backdrop: ArtworkUi? = null,
  val logo: ArtworkUi? = null,
  val actionItemId: String = id,
  val genres: List<String> = emptyList(),
  val rating: String? = null,
  val runtimeMinutes: Int? = null,
  val resumeSeconds: Double = 0.0,
  val durationSeconds: Double? = null,
  val inWatchlist: Boolean = false,
  val playable: Boolean = true,
  val playTargetId: String? = null,
  val episodeCode: String? = null,
  val quality: List<String> = emptyList(),
  val audioLabel: String? = null,
  val subtitleLabel: String? = null,
  val cast: List<CastUi> = emptyList(),
  val related: List<MediaUi> = emptyList(),
  val seasons: List<SeasonUi> = emptyList(),
) {
  val progress: Float get() = durationSeconds?.takeIf { it > 0 && it.isFinite() }
    ?.let { (resumeSeconds / it).toFloat().coerceIn(0f, 1f) } ?: 0f
}
internal data class CastUi(val id: String, val name: String, val role: String, val artwork: ArtworkUi? = null)
internal data class SeasonUi(val id: String, val title: String)
internal data class HomeRowUi(val id: String, val title: String, val items: List<MediaUi>, val landscape: Boolean = false, val libraryId: String? = null)
internal enum class PersonalListKind(@StringRes val title: Int) { Watchlist(R.string.watchlist), Favorites(R.string.favorites) }
internal enum class PlayedFilter(@StringRes val title: Int) { All(R.string.all_media), Played(R.string.watched), Unplayed(R.string.unwatched) }
internal enum class LibrarySort(@StringRes val title: Int) { DateAdded(R.string.latest_media), Title(R.string.sort_title), Year(R.string.sort_year) }
internal enum class AccountPage(@StringRes val title: Int) {
  Overview(R.string.account), Settings(R.string.settings), CurrentAccount(R.string.account_current),
  Connections(R.string.saved_connections), Appearance(R.string.appearance), Subtitles(R.string.subtitle_tracks),
  Playback(R.string.playback_preferences), Storage(R.string.storage), History(R.string.watch_history), Diagnostics(R.string.diagnostics),
}
internal enum class ThemePreference(@StringRes val title: Int) { System(R.string.follow_system), Dark(R.string.dark_theme), Light(R.string.light_theme) }
internal enum class LanguagePreference(@StringRes val title: Int) { System(R.string.follow_system), English(R.string.english), Chinese(R.string.chinese) }
internal enum class IntroPreference(@StringRes val title: Int) { Auto(R.string.skip_automatic), Manual(R.string.skip_manual), Off(R.string.subtitles_off) }
internal data class PreferencesUi(
  val theme: ThemePreference = ThemePreference.System,
  val language: LanguagePreference = LanguagePreference.System,
  val reducedMotion: Boolean = false,
  val startupAutoLogin: Boolean = true,
  val targetName: String = "JellyPilot Android",
  val autoPlayNext: Boolean = true,
  val introMode: IntroPreference = IntroPreference.Manual,
  val preferredSubtitleLanguage: String = "",
  val preferOriginalAudio: Boolean = true,
  val rememberSeasonVolume: Boolean = true,
  val playerGestures: Boolean = true,
)
internal data class DetailTrackUi(val index: Int, val label: String, val language: String? = null, val codec: String? = null, val isDefault: Boolean = false, val isExternal: Boolean = false)
internal data class DetailTracksUi(
  val targetId: String,
  val audio: List<DetailTrackUi> = emptyList(),
  val subtitles: List<DetailTrackUi> = emptyList(),
  val selectedAudio: Int? = null,
  val selectedSubtitle: Int? = null,
  val busy: Boolean = false,
  val error: String? = null,
)
internal data class RecoveryUi(val title: String, val positionSeconds: Double, val itemId: String)
internal data class ListUndoUi(val id: Long, val count: Int, val busy: Boolean = false, val error: String? = null)
internal data class PlaybackUi(
  val title: String,
  val episodeLabel: String? = null,
  val queue: List<MediaUi> = emptyList(),
  val queueHasMore: Boolean = false,
  val queueLoading: Boolean = false,
  val currentItemId: String? = null,
  val autoSkipAvailable: Boolean = false,
  val autoSkipEnabled: Boolean = false,
  val manualSkipLabel: String? = null,
  val skipUndoAvailable: Boolean = false,
  val skipUndoId: Long = 0L,
  val canPrevious: Boolean = false,
  val canNext: Boolean = false,
)
internal data class LibraryUi(val id: String, val title: String)
internal data class ProfileUi(val key: String, val name: String, val server: String, val provider: String, val active: Boolean, val url: String = "")

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

internal enum class LoginStep { Server, Account }

internal data class LoginServerUi(
  val name: String?,
  val address: String,
  val jellyfin: Boolean,
  val providerKnown: Boolean = true,
  val providerSelected: Boolean = true,
)

/** A projection for Compose; operation tokens, active scopes and user-data decisions stay in Rust. */
internal data class AppUiState(
  val destination: Destination = Destination.Home,
  val profiles: List<ProfileUi> = emptyList(),
  val activeName: String? = null,
  val activeProfileKey: String? = null,
  val selectedProfileKey: String? = null,
  val watchlistCleanupKeys: List<String> = emptyList(),
  val libraries: List<LibraryUi> = emptyList(),
  val libraryId: String? = null,
  val items: List<MediaUi> = emptyList(),
  val browser: BrowserUi = BrowserUi(),
  val detail: MediaUi? = null,
  val detailItems: List<MediaUi> = emptyList(),
  val detailTracks: DetailTracksUi? = null,
  val busy: Boolean = false,
  val loginBusy: Boolean = false,
  val quickConnectCode: String? = null,
  val error: String? = null,
  val notice: String? = null,
  val showSignIn: Boolean = false,
  val showPlayer: Boolean = false,
  /** Raw search field text; mirrors the retained SDK query so a recreated field never shows stale or empty input. */
  val searchQuery: String = "",
  /** SDK sign-out deleted the saved credentials but teardown failed; the session stays connected for a cleanup retry and new playback/writes stay blocked. */
  val signOutCleanupPending: Boolean = false,
  val featured: List<MediaUi> = emptyList(),
  val homeRows: List<HomeRowUi> = emptyList(),
  val selectedList: PersonalListKind = PersonalListKind.Watchlist,
  val listItems: List<MediaUi> = emptyList(),
  val listCount: Int = 0,
  val favoriteCount: Int = 0,
  val listBusy: Boolean = false,
  val listHasMore: Boolean = false,
  val historyHasMore: Boolean = false,
  val episodesHaveMore: Boolean = false,
  val listUndo: ListUndoUi? = null,
  val libraryPlayed: PlayedFilter = PlayedFilter.All,
  val libraryFavorites: Boolean = false,
  val librarySort: LibrarySort = LibrarySort.DateAdded,
  val accountPage: AccountPage = AccountPage.Overview,
  val preferences: PreferencesUi = PreferencesUi(),
  val historyItems: List<MediaUi> = emptyList(),
  val historyBusy: Boolean = false,
  val historyUndo: ListUndoUi? = null,
  val recovery: RecoveryUi? = null,
  val playbackUi: PlaybackUi? = null,
  val selectedSeasonId: String? = null,
  val loginServer: String = "",
  val loginUsername: String = "",
  val loginJellyfin: Boolean = true,
  val loginRemember: Boolean = true,
  val loginStep: LoginStep = LoginStep.Server,
  val loginIdentity: LoginServerUi? = null,
  /** Memory-only form input; never copied into SavedInstanceState or login prefill. */
  val loginPassword: String = "",
  val loginError: String? = null,
  val loginConnectionLost: Boolean = false,
  val loginPublicInfoRestricted: Boolean = false,
  /** Candidate adoption is atomic; Back cancels network work before this short commit phase. */
  val loginCommitting: Boolean = false,
)
