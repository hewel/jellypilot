# Treat cached Library Images as server-scoped non-sensitive data

JellyPilot will scope Library Image cache bytes and signed-reference authorization to the media server rather than the Saved Service Profile. Library permissions still govern browsing and playback, but image bytes are intentionally treated as non-sensitive: a same-server reference created under one profile may therefore return a cached image after another profile becomes active, avoiding profile-specific copies and access-grant metadata.
