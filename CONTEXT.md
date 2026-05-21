# JMSR Context

JMSR is a Jellyfin companion app that presents itself as a controllable playback target while delegating media playback to an external player.

## Language

**Server URL**:
The address of one Jellyfin server that JMSR connects to. A Server URL must be known before a user can authenticate with that server.
_Avoid_: Server discovery, server selection

**Playback Target**:
The JMSR install as it appears to Jellyfin users when they choose where media should play. The Playback Target should be identified by the configured device name.
_Avoid_: Generic app instance

**Quick Connect**:
A Jellyfin authentication method on the Login screen where JMSR shows a short code for the user to approve from another signed-in Jellyfin client. Quick Connect is the default login method and authenticates to a known Server URL; it does not discover or choose servers.
_Avoid_: Discovery, pairing, device link

**Quick Connect Request**:
A user-started login attempt against a known Server URL that produces a short code for approval in Jellyfin. While a Quick Connect Request is waiting, its Server URL is fixed until the user cancels the request.
_Avoid_: Auto-started login, background pairing

**Quick Connect Code**:
The short server-issued code JMSR displays during a Quick Connect Request. The code is approved from another signed-in Jellyfin client.
_Avoid_: Pairing code, device link code

**Quick Connect Approval**:
The point where a signed-in Jellyfin user authorizes the displayed Quick Connect code. JMSR waits for this approval automatically after showing the code.
_Avoid_: Manual confirmation

**Quick Connect Failure**:
A Quick Connect Request that cannot finish because it expires, is denied, or is rejected by the server. A Quick Connect Failure keeps the user in the Quick Connect flow until they retry or choose Password Login.
_Avoid_: Automatic password fallback

**Saved Session**:
An authenticated Jellyfin session that JMSR can restore after restart without asking the user to log in again. Saved Sessions are created the same way after Quick Connect and Password Login.
_Avoid_: Remembered password

**Login Prefill**:
Remembered unauthenticated login inputs, such as Server URL and username, used to make Password Login easier. Login Prefill is separate from a Saved Session.
_Avoid_: Saved Session, remembered password

**Password Login**:
A fallback authentication method where the user signs in to a known Server URL with Jellyfin username and password.
_Avoid_: Primary login

**Login Method**:
One of the user-selectable ways to authenticate to a known Server URL. Quick Connect and Password Login are the Login Methods currently exposed by JMSR.
_Avoid_: Account type, server type

**Intro Skipper**:
A Jellyfin server plugin that detects intro and credit ranges for media items so a Playback Target can skip those ranges during playback. In JMSR, Intro Skipper refers to the plugin-provided ranges, not Jellyfin media segments in general.
_Avoid_: Media Segment Skipping, chapter skipping, generic skip markers

**Automatic Intro Skip**:
JMSR advancing playback past an Intro Skipper range without asking the user for confirmation. Automatic Intro Skip is a playback behavior of the Playback Target, not an MPV overlay or prompt, and each fetched range is skipped at most once per playback session.
_Avoid_: Skip prompt, countdown, overlay

**Credit Skip**:
JMSR advancing playback past an Intro Skipper credit range. Credit Skip does not directly start the next episode; if the skip reaches natural end of playback, JMSR's normal next-episode behavior applies.
_Avoid_: Next episode command, outro button

**Intro Skipper Setting**:
A global user preference that controls whether JMSR uses Intro Skipper ranges during playback. The Intro Skipper Setting defaults to enabled so plugin ranges are used unless the user turns the behavior off.
_Avoid_: Plugin install state, server setting

**Diagnostics**:
A user-facing support view that shows sanitized JMSR runtime events useful for understanding Jellyfin connection, Playback Target, and external player problems. Diagnostics are not a developer console and should not expose arbitrary frontend console output or secret-bearing values.
_Avoid_: Frontend logs, debug console, telemetry

## Example Dialogue

Dev: "Can Quick Connect find my Jellyfin server?"

Domain expert: "No. The user first supplies the Server URL, then Quick Connect can authenticate JMSR with that server."

Dev: "Which JMSR install am I approving?"

Domain expert: "The Playback Target named by this install's configured device name."

Dev: "What if Quick Connect is disabled on the server?"

Domain expert: "The user toggles to Password Login and signs in with their Jellyfin credentials."

Dev: "Can I start Quick Connect from Settings?"

Domain expert: "No. Disconnect first, then authenticate again from the Login screen."

Dev: "When does JMSR ask the server for a Quick Connect code?"

Domain expert: "Only after the user confirms the Server URL by starting a Quick Connect Request."

Dev: "Can the user edit the Server URL while waiting for approval?"

Domain expert: "No. The request belongs to that Server URL, so the user cancels it before changing servers."

Dev: "After the code is shown, does the user need to tell JMSR they approved it?"

Domain expert: "No. JMSR waits for Quick Connect Approval automatically and then finishes the login."

Dev: "Can the user switch Login Methods while a Quick Connect Code is waiting?"

Domain expert: "No. The user cancels the current Quick Connect Request before choosing another Login Method."

Dev: "If the Quick Connect code expires, should JMSR switch to Password Login?"

Domain expert: "No. JMSR explains the Quick Connect Failure and lets the user retry or explicitly choose Password Login."

Dev: "Is Quick Connect a temporary one-time login?"

Domain expert: "No. After approval, JMSR creates a Saved Session just like Password Login."

Dev: "Does Quick Connect need a remember-me checkbox?"

Domain expert: "No. Quick Connect creates a Saved Session after approval; Login Prefill only applies to Password Login."

Dev: "Is Intro Skipper just any Jellyfin media segment?"

Domain expert: "No. For this feature, Intro Skipper means ranges supplied by the Intro Skipper plugin specifically."

Dev: "Should JMSR show a skip button over MPV?"

Domain expert: "No. Automatic Intro Skip means JMSR skips the plugin range silently."

Dev: "If the user seeks back into a skipped intro, should JMSR skip it again?"

Domain expert: "No. Each fetched Intro Skipper range is skipped at most once for that playback session."

Dev: "Does skipping credits mean JMSR immediately starts the next episode?"

Domain expert: "No. Credit Skip advances past the credit range; next-episode playback is still driven by natural end of playback."

Dev: "If the server has Intro Skipper ranges, are skips mandatory?"

Domain expert: "No. The Intro Skipper Setting lets the user turn automatic skipping off in JMSR."
