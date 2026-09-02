# Ship a paired, provenance-verified local media toolchain

_Status: Accepted. Refines the packaging and licensing decision in ADR 0019._

Embedded Web Playback depends on both media conversion and reliable source
inspection. JellyPilot will therefore treat FFmpeg and FFprobe as one **Local
Media Toolchain**: a target-specific, version-matched pair that is prepared,
verified, packaged, and licensed together. The application will not discover a
different FFmpeg or FFprobe from the host `PATH` as an implicit fallback.

## Target manifest and packaging

The checked-in sidecar manifest is the release input for every supported Rust
target. Each target entry identifies the exact FFmpeg executable, FFprobe
executable, shared license, available build metadata, and SHA-256 checksum for
every file. Preparation either reads those files from an explicit packaging
directory or downloads them from the recorded versioned release, verifies every
checksum, and writes the target-suffixed Tauri sidecars atomically. Verification
mode must fail when either executable or either notice file is absent or has a
different checksum.

Tauri bundles both executables as external binaries. Native release jobs prepare
and verify the pair before building. The Arch recipe sources and checksums both
executables explicitly, installs them beside JellyPilot under
`/usr/lib/jellypilot`, and installs the manifest, shared license, build metadata,
and third-party notice. Updating the repository recipe does not itself publish
the separate AUR repository.

## Active compatibility baseline

Until a JellyPilot-owned FFmpeg 9.0 build is published and passes the cutover
gates below, all normal builds retain the paired files from the existing
`eugeneware/ffmpeg-static` `b6.1.1` release. The release label does not prove a
uniform FFmpeg version across platforms: upstream used different build
providers, and some uploaded README metadata is floating or inconsistent. The
manifest's checksums identify the selected upload bytes; the executable's own
version and build configuration remain authoritative.

This baseline release is not marked immutable by its upstream repository.
Mandatory checksum verification limits substitution, but cannot guarantee the
continued availability of the upstream downloads. That is an explicit reason
to move Linux x86_64 to a JellyPilot-owned immutable release rather than an
excuse to point normal builds at assets that do not exist yet.

## JellyPilot-owned Linux x86_64 build

The dedicated manual and version-tag workflow builds a GPL Linux x86_64 pair
from these pinned inputs:

- the official signed FFmpeg `n9.0` release, resolved to commit
  `d32b387f2b0a484599d4587d651891f0c63c4238`;
- BtbN `FFmpeg-Builds` commit
  `2437e7b868da3c11872367b15f3c613b87c24819`, whose tree is
  `9484782a760055d99e8c2b2a4ebbf2e9ead596e6`;
- the `linux64 gpl 9.0` build variant with GPLv3/version3, VAAPI, libdrm,
  libx264, and libx265 enabled.

`--enable-nonfree` is forbidden. The workflow verifies the FFmpeg release
signature and resolved commit, verifies the BtbN commit and tree, captures both
tools' version output and FFmpeg's build configuration, checks the required
software and VAAPI encoders, exercises FFprobe against generated media, and
performs a fragmented-MP4 HLS smoke test. A hosted runner can prove that VAAPI
support and encoders are present, but cannot prove hardware encoding without a
GPU device and driver; hardware execution remains a separate GPU-runner gate.

The workflow emits the paired executables, GPL license, build information,
machine-readable provenance, and SHA-256 checksums. A manual run uploads a
workflow artifact for inspection. A version tag matching
`ffmpeg-n9.0-linux-x86_64-r*` may publish those files only after GitHub immutable
releases are enabled for the repository; publication uses a draft so all assets
are attached before the release is locked.

## Cutover gate

The active Linux x86_64 manifest and Arch recipe may switch away from `b6.1.1`
only after all of the following are true:

1. A manual workflow run completes all source, configuration, encoder, probe,
   and HLS checks.
2. GitHub release immutability is enabled for `hewel/jellypilot`.
3. A version tag publishes the JellyPilot-owned pair, license, build information,
   provenance, and checksums as an immutable release.
4. The published asset digests are independently compared with `SHA256SUMS` and
   then recorded in the manifest and Arch recipe using the immutable release
   URL.
5. Fresh prepare/verify runs and the normal Linux and Arch release builds pass
   using only those published assets.

Until then, keeping the checked-in `b6.1.1` pair is required for usable developer
and release builds.

## Consequences

- FFmpeg and FFprobe cannot drift independently within a packaged target.
- Missing or altered media tools fail the build instead of silently changing
  runtime behavior.
- GPL builds ship the corresponding GPLv3 license, build details, source/build
  provenance, and checksums while JellyPilot itself remains MIT-licensed and
  invokes the tools as separate processes.
- The first JellyPilot-owned build covers Linux x86_64 only; the manifest keeps
  the current verified pair for other supported targets until equivalent owned
  builds are produced.
- A versioned immutable release is a supply-chain boundary. Its assets and tag
  must never be repurposed for a later build; changes require a new release
  revision.
