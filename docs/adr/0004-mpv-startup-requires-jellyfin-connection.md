# Require Jellyfin connection before MPV startup

JMSR starts MPV only from Now Playing and only while a live Jellyfin connection exists. This keeps the Playback Target lifecycle tied to an authenticated server session, avoids presenting MPV as useful without Jellyfin cast/control context, and makes disconnected recovery explicit through the Connection card instead of letting users spawn an unmanaged external player.
