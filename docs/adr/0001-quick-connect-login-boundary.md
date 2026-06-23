# Quick Connect stays inside login

Quick Connect authenticates JellyPilot to a known single Jellyfin Server URL from the Login screen. It does not discover servers or select between servers inside the Quick Connect flow, because this keeps the first Quick Connect slice aligned with explicit user-entered server URLs. Richer server and account management is handled separately through Saved Service Profiles; see ADR 0009.
