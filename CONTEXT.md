# JellyPilot Context

JellyPilot is a Jellyfin and Emby companion app that presents itself as a controllable Playback Target, browses video libraries, and plays media through a standalone MPV process.

These definitions describe accepted product semantics. Delivery status for the Sidebar, Personal Lists, and account changes is tracked in the [native sidebar specification](docs/sidebar-design-spec.md).

## Language

**Server URL**:
The address of one Jellyfin or Emby server that JellyPilot connects to. A Server URL must be known before a user can authenticate with that server.
_Avoid_: Server discovery, server selection

**Playback Target**:
The JellyPilot install as it appears to media-server users when they choose where media should play. The Playback Target is identified by the configured device name.
_Avoid_: Generic app instance

**Playback Session**:
One active presentation of a media item through External MPV Playback.
_Avoid_: Player process, transcode job

**External MPV Playback**:
Playback presented by a standalone MPV process so the user's MPV configuration, scripts, and shaders remain available.
_Avoid_: Embedded MPV, libmpv

**Provider Transcode**:
Media conversion performed by the connected Jellyfin or Emby server. JellyPilot plays the original or direct source through External MPV Playback and does not request a Provider Transcode.

**Quick Connect**:
A Jellyfin authentication method where JellyPilot shows a short code for the user to approve from another signed-in Jellyfin client. Quick Connect is the default Jellyfin login method and authenticates to a known Server URL; it does not discover or choose servers.
_Avoid_: Discovery, pairing, device link

**Quick Connect Request**:
A user-started login attempt against a known Server URL that produces a short code for approval in Jellyfin. While a Quick Connect Request is waiting, its Server URL is fixed until the user cancels the request.
_Avoid_: Auto-started login, background pairing

**Quick Connect Code**:
The short server-issued code JellyPilot displays during a Quick Connect Request. The code is approved from another signed-in Jellyfin client.
_Avoid_: Pairing code, device link code

**Quick Connect Approval**:
The point where a signed-in Jellyfin user authorizes the displayed Quick Connect code. JellyPilot waits for this approval automatically after showing the code.
_Avoid_: Manual confirmation

**Quick Connect Failure**:
A Quick Connect Request that cannot finish because it expires, is denied, or is rejected by the server. A Quick Connect Failure keeps the user in the Quick Connect flow until they retry or choose Password Login.
_Avoid_: Automatic password fallback

**Saved Service Profile**:
An authenticated media-server profile that JellyPilot can restore after restart without asking the user to log in again. Saved Service Profiles are created the same way after Quick Connect and Password Login, and JellyPilot keeps at most one active profile at a time.
_Avoid_: Remembered password

**Profile Scope**:
The combination of media-server provider, Server URL, and server user identity that owns one user's JellyPilot content. Display names do not determine Profile Scope, and the same user identity on different servers belongs to different scopes.
_Avoid_: Username as account identity, server name as account identity

**Profile Switch**:
Replacing the active media-server connection with another authenticated profile. A successful Profile Switch ends the old Playback Session; a target authentication failure leaves the current connection and Playback Session in place.
_Avoid_: Parallel server connection, App Mode switch

**Startup Auto Login**:
The preference to restore the last successfully activated Saved Service Profile when JellyPilot starts. Startup Auto Login is distinct from reconnecting the remote Playback Target after a network interruption.
_Avoid_: Network auto-reconnect, first saved account

**Disconnect**:
Ending JellyPilot's current live media-server connection while keeping saved service profiles available for later reconnect.
_Avoid_: Sign out, clear session

**Sign Out**:
Removing a selected Saved Service Profile from this device and ending the live connection if that profile is active. Other saved profiles are unaffected, and deleting the selected profile's local Watchlist is a separate, explicit choice.
_Avoid_: Temporary disconnect, server account deletion, server-side token revocation

**Login Prefill**:
Remembered unauthenticated login inputs, such as Server URL and username, used to make Password Login easier. Login Prefill is separate from a Saved Service Profile.
_Avoid_: Saved Service Profile, remembered password

**Password Login**:
A method where the user signs in to a known Server URL with that server's username and password. Password Login is the fallback to Quick Connect for Jellyfin and the login method for Emby.
_Avoid_: Remembered password

**Login Method**:
One of the user-selectable ways to authenticate to a known Server URL. Quick Connect and Password Login are the Login Methods currently exposed by JellyPilot.
_Avoid_: Account type, server type

**Now Playing**:
The user-facing playback status shown by JellyPilot for the current Playback Session. Now Playing may show transport state before rich media-server metadata is available.
_Avoid_: MPV state, Web player state, playback session internals

**App Mode**:
The persisted top-level operating mode of JellyPilot: Full or Control-Only. App Mode decides which UI surfaces exist and how the window behaves; it switches live from Settings without restarting JellyPilot or interrupting the current Playback Session.
_Avoid_: View preference, layout setting, window profile

**Control-Only Mode**:
The lowest-idle-overhead media-controller App Mode: a fixed-size 480x760 window centered on Now Playing, with every Login Method and the Settings Modal, plus the tray and the remote Playback Target. Control-Only Mode has no Library Browser; closing its window leaves the tray and Playback Target available, and entering the mode drops Library Browser state.
_Avoid_: Mini player, compact view, floating widget

**Library Browser**:
The authenticated JellyPilot shell area for browsing video libraries, inspecting item details, launching Playback Sessions, and applying user-scoped media state. Library Browser complements the Playback Target; it is not a goal to replace every Jellyfin or Emby client feature.
_Avoid_: Full media-server replacement

**Library Image**:
A still image shown in the Library Browser for media content. Library Images include Artwork and Backdrop and are not limited to portrait posters.
_Avoid_: Poster as the umbrella term, thumbnail

**Library Image Cache**:
A best-effort disk copy of a media server's original Library Image response bytes, shared by Saved Service Profiles that refer to the same server. The Library Image Cache accelerates repeat browsing but is never a transformed representation or an offline source of truth.
_Avoid_: Image optimizer, offline artwork library, Saved Service Profile cache

**Library Image Raster**:
An in-memory, display-sized RGBA decode of a Library Image, keyed by the Library Image reference and a size class. Library Image Rasters accelerate first paint and repeat rendering across navigations; they are never persisted and are distinct from the Library Image Cache, which stores only origin-encoded bytes.
_Avoid_: Texture, transformed cache variant, decoded cache entry

**Episode Still**:
An episode's Primary Library Image: a landscape frame taken from the episode itself. Episode Stills are the preferred card image for episode items in Continue Watching and Next Up; when an Episode Still is missing, the card falls back to the episode's Thumb-type Library Image, then the series Thumb, then the series Backdrop. A portrait artwork slot never uses an Episode Still.
_Avoid_: Screenshot, episode thumbnail as the preferred card image

**Title Logo**:
A Logo-type Library Image: a transparent title treatment for a movie or series. Heroes show the Title Logo in place of the text headline when one is available; an episode detail hero shows the parent series' Title Logo alongside the episode title text. Title Logos keep their aspect and alpha: they are never cropped or corner-rounded, and heroes render them with a soft drop shadow baked from the logo's alpha so light logos stay legible over bright Backdrops. Hero display size is normalized by visual area: wide logos fit the hero's reference height while narrower marks are boosted toward a shared area target, so small logos keep weight next to wide ones. When no Title Logo exists, the hero falls back to the text headline.
_Avoid_: Poster substitute, watermark, app icon

**Sidebar**:
The persistent left navigation area of the authenticated Full-mode shell, providing Video Home, Personal Lists, video libraries, and account and application controls. At narrow window widths the Sidebar shows icons only; Control-Only Mode has no Sidebar.
_Avoid_: Navigation rail, app drawer, floating controls

**Account Popover**:
The Sidebar's account surface for the active connection and saved logins, including Profile Switch, adding a login, Disconnect, and Sign Out. It presents the same account-management capabilities that remain available through Settings in Control-Only Mode.
_Avoid_: Server cluster, member directory

**Settings Modal**:
The closable settings layer presented over the current shell context, centered in wide windows and filling narrow windows. The Settings Modal is dismissed via the Esc key or the close (✕) button and is never a navigation destination or stack entry.
_Avoid_: Settings page, settings destination, dialog popup, drawer, click-outside-to-close

**Video Home**:
The Library Browser landing view built from live media-server rows such as Continue Watching, Next Up, latest Movies, latest Episodes, and video library shortcuts. Video Home belongs to the current Profile Scope and is not cached offline.
_Avoid_: Home page, dashboard mock data

**Personal Lists**:
The Library Browser destination, labeled “我的清单”, that presents Favorites and Watchlist as separate sections for the current Profile Scope. Both sections include movies, series, and individual episodes.
_Avoid_: Cross-server collection, merged favorite/watchlist state

**Favorites**:
The current server user's expression of liking a media item, shared through Jellyfin or Emby with other clients. Favorites are independent of Watchlist membership and watched status.
_Avoid_: Watchlist, planned viewing

**Watchlist**:
A viewing plan kept on this device for one Profile Scope, labeled “稍后观看”. Its entries remain until explicitly removed, including after viewing or when an item becomes unavailable; Sign Out retains the list unless the user explicitly chooses to delete it.
_Avoid_: Favorites, unwatched filter, cross-device list

**Featured Item**:
The Continue Watching item with a resume position that is presented as the Video Home hero; when nothing is resumable, the first Next Up item, then the first item of a later home row. The Featured Item's Backdrop fills the hero background, and its Title Logo (the parent series' Title Logo for episodes) serves as the hero headline. Heroes have no portrait poster slot.
_Avoid_: Spotlight, hero carousel item

**User Data Action**:
A user-scoped Jellyfin or Emby mutation for item state such as favorite, unfavorite, mark played, or mark unplayed. User Data Actions update visible Library Browser state only after the server accepts the mutation.
_Avoid_: Optimistic toggle, local-only media state

**Intro Skipper**:
A Jellyfin server plugin that detects intro and credit ranges for media items so a Playback Target can skip those ranges during playback. In JellyPilot, Intro Skipper refers to the plugin-provided ranges, not Jellyfin media segments in general.
_Avoid_: Media Segment Skipping, chapter skipping, generic skip markers

**Automatic Intro Skip**:
JellyPilot advancing playback past an Intro Skipper range without asking the user for confirmation. Automatic Intro Skip is a playback behavior of the Playback Target, not an MPV overlay or prompt, and each fetched range is skipped at most once per playback session.
_Avoid_: Skip prompt, countdown, overlay

**Credit Skip**:
JellyPilot advancing playback past an Intro Skipper credit range. Credit Skip does not directly start the next episode; if the skip reaches natural end of playback, JellyPilot's normal next-episode behavior applies.
_Avoid_: Next episode command, outro button

**Intro Skipper Setting**:
A global user preference that controls whether JellyPilot uses Intro Skipper ranges during playback. The Intro Skipper Setting defaults to enabled so plugin ranges are used unless the user turns the behavior off.
_Avoid_: Automation, Playback automation, Plugin install state, server setting

**Diagnostics**:
A user-facing support view that shows sanitized JellyPilot runtime events useful for understanding Jellyfin connection, Playback Target, and external player problems. Diagnostics are not a developer console and should not expose arbitrary frontend console output or secret-bearing values.
_Avoid_: Frontend logs, debug console, telemetry

**Direct Playback**:
Launching playback of a Library Browser item immediately when the user presses Play, Resume, or Play from beginning, without an intermediate track-selection dialog. JellyPilot sends null audio and subtitle stream indices so the backend applies its own preference resolution (series preferences, global language preferences, defaults). Track switching during an active session remains available through the Now Playing controls.
_Avoid_: Track chooser dialog, pre-playback modal, stream picker

## Example Dialogue

Dev: "Can Quick Connect find my Jellyfin server?"

Domain expert: "No. The user first supplies the Server URL, then Quick Connect can authenticate JellyPilot with that server."

Dev: "Which JellyPilot install am I approving?"

Domain expert: "The Playback Target named by this install's configured device name."

Dev: "What if Quick Connect is disabled on the server?"

Domain expert: "The user toggles to Password Login and signs in with their Jellyfin credentials."

Dev: "Must I disconnect before adding another Jellyfin login?"

Domain expert: "No. Connect and switch authenticates the new login while keeping the current connection until the target is ready. An active Playback Session requires confirmation before this flow."

Dev: "When does JellyPilot ask the server for a Quick Connect code?"

Domain expert: "Only after the user confirms the Server URL by starting a Quick Connect Request."

Dev: "Can the user edit the Server URL while waiting for approval?"

Domain expert: "No. The request belongs to that Server URL, so the user cancels it before changing servers."

Dev: "After the code is shown, does the user need to tell JellyPilot they approved it?"

Domain expert: "No. JellyPilot waits for Quick Connect Approval automatically and then finishes the login."

Dev: "Can the user switch Login Methods while a Quick Connect Code is waiting?"

Domain expert: "No. The user cancels the current Quick Connect Request before choosing another Login Method."

Dev: "If the Quick Connect code expires, should JellyPilot switch to Password Login?"

Domain expert: "No. JellyPilot explains the Quick Connect Failure and lets the user retry or explicitly choose Password Login."

Dev: "Is Quick Connect a temporary one-time login?"

Domain expert: "No. After approval, JellyPilot creates a Saved Service Profile just like Password Login."

Dev: "Does Quick Connect need a remember-me checkbox?"

Domain expert: "No. Quick Connect creates a Saved Service Profile after approval; Login Prefill only applies to Password Login."

Dev: "Does restoring a Saved Service Profile fill in my password?"

Domain expert: "No. A Saved Service Profile restores an authenticated login. Login Prefill remembers the server and username, never the password."

Dev: "Will a failed Profile Switch stop my current movie?"

Domain expert: "Target authentication failure leaves the current connection and movie in place. A successful switch ends that Playback Session before the new profile becomes active."

Dev: "Does Control-Only Mode switch my media-server account?"

Domain expert: "No. An App Mode switch keeps the active profile and Playback Session. A Profile Switch changes the active account."

Dev: "Is a movie removed from Watchlist when I finish watching it?"

Domain expert: "No. Watchlist is your viewing plan, including things you may want to rewatch. Remove the movie explicitly when you no longer want it there."

Dev: "Will my second computer see this Watchlist?"

Domain expert: "No. Each device keeps its own Watchlist for that Profile Scope. Favorites belong to the server user and can be shared with other clients."

Dev: "Can I sign out another saved account without switching to it?"

Domain expert: "Yes. Sign Out removes that device's saved login without disturbing the active account. You can separately choose to delete the selected account's local Watchlist."

Dev: "Is Intro Skipper just any Jellyfin media segment?"

Domain expert: "No. For this feature, Intro Skipper means ranges supplied by the Intro Skipper plugin specifically."

Dev: "Should JellyPilot show a skip button over MPV?"

Domain expert: "No. Automatic Intro Skip means JellyPilot skips the plugin range silently."

Dev: "If the user seeks back into a skipped intro, should JellyPilot skip it again?"

Domain expert: "No. Each fetched Intro Skipper range is skipped at most once for that playback session."

Dev: "Does skipping credits mean JellyPilot immediately starts the next episode?"

Domain expert: "No. Credit Skip advances past the credit range; next-episode playback is still driven by natural end of playback."

Dev: "If the server has Intro Skipper ranges, are skips mandatory?"

Domain expert: "No. The Intro Skipper Setting lets the user turn automatic skipping off in JellyPilot."

Dev: "Should JellyPilot ask which audio track to use before starting playback?"

Domain expert: "No. Direct Playback starts immediately with backend preference resolution; the user can switch tracks from Now Playing while the session is active."

Dev: "Does the Library Image Cache store decoded rasters?"

Domain expert: "No. The Library Image Cache stores only origin-encoded bytes on disk; Library Image Rasters are display-sized decodes that live in memory and are never persisted."

Dev: "Is a Library Image Raster a new kind of Library Image reference?"

Domain expert: "No. A Library Image Raster is keyed by an existing Library Image reference plus a render-side size class; it does not change what is requested from the server."

Dev: "Which image leads an episode card in Continue Watching or Next Up?"

Domain expert: "The Episode Still. The episode Thumb, series Thumb, and series Backdrop are only fallbacks when the still is missing."

Dev: "Does the Video Home hero show a poster for the Featured Item?"

Domain expert: "No. Heroes show the Title Logo over the Backdrop and fall back to the text headline when no Title Logo exists; Episode Stills are landscape imagery and appear only in landscape card slots."
