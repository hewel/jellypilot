# Separate server Favorites from the device-local Watchlist

_Status: Accepted, 2026-09-05. Feature implementation and acceptance remain pending in the [native sidebar specification](../sidebar-design-spec.md)._

Favorites express liking an item; Watchlist expresses a viewing plan, so treating the server's
favorite flag as “watch later” would collapse two independent user intentions. Personal Lists
presents both in one destination, as separate sections, within the active Profile Scope.

Keep Favorites owned by the Jellyfin or Emby user and keep Watchlist on this device, scoped by
provider, Server URL, and server user identity. Watchlist supports movies, series, and episodes;
it records explicit membership independently of watched status or current server availability.
This preserves Favorites interoperability without introducing a server extension or external
synchronization service for planned viewing. The deliberate cost is that Watchlist does not
synchronize between devices.

Watchlist has its own durable records, separate from saved authentication secrets and the
best-effort Library Image Cache. Signing out retains those records unless the user explicitly
chooses to delete the selected profile's Watchlist; unavailable items remain identifiable and
removable. Future cross-device synchronization would require an explicit ownership and migration
decision rather than silently changing this local-data contract.
