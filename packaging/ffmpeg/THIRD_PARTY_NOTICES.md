# FFmpeg sidecar notice

JellyPilot distributions bundle an unmodified FFmpeg executable selected by
[`ffmpeg-static` 5.3.0](https://www.npmjs.com/package/ffmpeg-static/v/5.3.0). The
package source is pinned to commit
[`24504b7d549c0400e099720b3d5854577693a63a`](https://github.com/eugeneware/ffmpeg-static/tree/24504b7d549c0400e099720b3d5854577693a63a),
and the binary assets come from its `b6.1.1` release. Exact artifact names and
SHA-256 checksums are recorded in `manifest.json`.

FFmpeg and the libraries linked into each static build are free software. The
license text supplied with the selected upstream build is distributed beside
this notice as `LICENSE.txt`, and its component/version inventory is distributed
as `BUILD-INFO.txt`.

Corresponding source and build provenance are available from:

- FFmpeg source releases: <https://ffmpeg.org/releases/>
- ffmpeg-static acquisition/build metadata: <https://github.com/eugeneware/ffmpeg-static/tree/24504b7d549c0400e099720b3d5854577693a63a>
- Linux static-build sources: <https://johnvansickle.com/ffmpeg/>
- Windows static-build sources: <https://www.gyan.dev/ffmpeg/builds/>
- Intel macOS static-build sources: <https://evermeet.cx/ffmpeg/>
- Apple Silicon macOS static-build sources: <https://www.osxexperts.net/>

Use `BUILD-INFO.txt` to identify the exact FFmpeg and linked-library versions in
the binary shipped for a particular platform.
