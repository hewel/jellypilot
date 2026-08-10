# FFmpeg and FFprobe sidecar notice

JellyPilot distributions bundle paired, unmodified FFmpeg and FFprobe executables selected by
[`ffmpeg-static` 5.3.0](https://www.npmjs.com/package/ffmpeg-static/v/5.3.0). The
package source is pinned to commit
[`24504b7d549c0400e099720b3d5854577693a63a`](https://github.com/eugeneware/ffmpeg-static/tree/24504b7d549c0400e099720b3d5854577693a63a),
and the current binary assets come from its `b6.1.1` release. Each target's
FFmpeg and FFprobe executables come from the same release upload. Exact artifact
names and publisher-provided SHA-256 checksums are recorded in `manifest.json`
and enforced before packaging.

The `b6.1.1` release is the active compatibility baseline, not a claim that every
uploaded executable reports FFmpeg 6.1.1. Upstream used different platform build
providers, and some supplied build metadata is floating or inconsistent. Treat
the recorded artifact checksums and each executable's own `-version` and
`-buildconf` output as authoritative for the bytes in a package.

FFmpeg and the libraries linked into each static build are free software. The
license text supplied with the selected upstream build is distributed beside
this notice as `LICENSE.txt`, and the available component/version inventory is
distributed as `BUILD-INFO.txt`. The files apply to the paired executables for
the selected target.

Corresponding source and build provenance are available from:

- FFmpeg source releases: <https://ffmpeg.org/releases/>
- ffmpeg-static acquisition/build metadata: <https://github.com/eugeneware/ffmpeg-static/tree/24504b7d549c0400e099720b3d5854577693a63a>
- Linux static-build sources: <https://johnvansickle.com/ffmpeg/>
- Windows static-build sources: <https://www.gyan.dev/ffmpeg/builds/>
- Intel macOS static-build sources: <https://evermeet.cx/ffmpeg/>
- Apple Silicon macOS static-build sources: <https://www.osxexperts.net/>

The dedicated FFmpeg sidecar workflow is the provenance path for the planned
JellyPilot-owned Linux x86_64 FFmpeg 9.0 GPL pair. It verifies the official
signed release, pins the FFmpeg and BtbN build-system commits, records the build
configuration, runs encode/probe/HLS smoke tests, and emits checksums and
provenance beside the executables. The active manifest must not switch to those
assets until that workflow has published an immutable release and its resulting
checksums have been reviewed into `manifest.json`.
