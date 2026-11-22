#!/usr/bin/env bash
set -euo pipefail

# Build only native x86_64 Jammy binaries. Installing each source build as a local
# Debian package lets linux-runtime retain exact package versions and copyright
# notices, just as it does for the signed distribution dependencies.
test "$(dpkg --print-architecture)" = amd64
work=/opt/jellypilot-native-sources
mkdir -p "$work"
cd "$work"

checkout() {
  local name="$1" remote="$2" revision="$3"
  git init "$name"
  git -C "$name" remote add origin "$remote"
  git -C "$name" fetch --depth=1 origin "$revision"
  git -C "$name" checkout --detach FETCH_HEAD
  test "$(git -C "$name" rev-parse HEAD)" = "$revision"
}

package() {
  local name="$1" version="$2" source="$3"
  shift 3
  local doc="$work/stage/usr/share/doc/$name"
  mkdir -p "$work/stage/DEBIAN" "$doc"
  printf 'Package: %s\nVersion: %s\nArchitecture: amd64\nMaintainer: JellyPilot builders <noreply@jellypilot.invalid>\nDescription: Native Jammy build of pinned %s for the JellyPilot AppImage\n' \
    "$name" "$version" "$source" > "$work/stage/DEBIAN/control"
  {
    printf 'Upstream source: %s\nRevision: %s\n\n' \
      "$(git -C "$source" remote get-url origin)" "$(git -C "$source" rev-parse HEAD)"
    printf 'Build recipe (including configure options):\n'
    cat /opt/build-appimage-deps.sh
    for license in "$@"; do
      printf '\n===== %s =====\n' "$license"
      cat "$source/$license"
    done
  } > "$doc/copyright"
  dpkg-deb --build --root-owner-group "$work/stage" "$work/$name.deb"
  dpkg -i "$work/$name.deb"
  rm -rf "$work/stage" "$work/$name.deb"
  ldconfig
}

case "${1:?Specify shaderc, wayland, protocols, vulkan or ffmpeg}" in
shaderc)
# Use shaderc's own DEPS pins for the complete compiler stack, including glslang
# used by libplacebo. Jammy has no libshaderc-dev package.
checkout shaderc https://github.com/google/shaderc.git 0968768c61d4eb7dd861114412e904bb3d59b7b6
checkout shaderc/third_party/glslang https://github.com/KhronosGroup/glslang.git 8b822ee8ac2c3e52926820f46ad858532a895951
checkout shaderc/third_party/spirv-headers https://github.com/KhronosGroup/SPIRV-Headers.git 54a521dd130ae1b2f38fef79b09515702d135bdd
checkout shaderc/third_party/spirv-tools https://github.com/KhronosGroup/SPIRV-Tools.git f289d047f49fb60488301ec62bafab85573668cc
cmake -S shaderc -B shaderc-build -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local \
  -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DSHADERC_SKIP_TESTS=ON -DSHADERC_SKIP_EXAMPLES=ON \
  -DSHADERC_SKIP_COPYRIGHT_CHECK=ON -DSPIRV_SKIP_EXECUTABLES=ON \
  -DENABLE_GLSLANG_BINARIES=OFF
cmake --build shaderc-build --parallel "$(nproc)"
DESTDIR="$work/stage" cmake --install shaderc-build
package jellypilot-build-shaderc 2025.1 shaderc LICENSE \
  third_party/glslang/LICENSE.txt third_party/spirv-tools/LICENSE third_party/spirv-headers/LICENSE
;;
wayland)
checkout wayland https://gitlab.freedesktop.org/wayland/wayland.git a9fec8dd65977c57f4039ced34327204d9b9d779
meson setup wayland-build wayland --prefix=/usr/local --libdir=lib \
  --buildtype=release --wrap-mode=nofallback -Ddocumentation=false -Dtests=false
meson compile -C wayland-build
DESTDIR="$work/stage" meson install -C wayland-build
package jellypilot-build-wayland 1.23.1 wayland COPYING
;;
protocols)

# mpv's Wayland color path uses the color-representation protocol added in 1.44.
checkout wayland-protocols https://gitlab.freedesktop.org/wayland/wayland-protocols.git 810f1adaf33521cc55fc510566efba2a1418174f
meson setup protocols-build wayland-protocols --prefix=/usr/local \
  --buildtype=release --wrap-mode=nofallback -Dtests=false
DESTDIR="$work/stage" meson install -C protocols-build
package jellypilot-build-wayland-protocols 1.44 wayland-protocols COPYING
;;
vulkan)

checkout vulkan-headers https://github.com/KhronosGroup/Vulkan-Headers.git 952f776f6573aafbb62ea717d871cd1d6816c387
cmake -S vulkan-headers -B headers-build -G Ninja -DCMAKE_INSTALL_PREFIX=/usr/local
DESTDIR="$work/stage" cmake --install headers-build
package jellypilot-build-vulkan-headers 1.4.309 vulkan-headers LICENSE.md LICENSES/Apache-2.0.txt LICENSES/MIT.txt

checkout vulkan-loader https://github.com/KhronosGroup/Vulkan-Loader.git 809941a4ca137df69dc9c6e8eb456bd70309197c
cmake -S vulkan-loader -B loader-build -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local \
  -DCMAKE_INSTALL_LIBDIR=lib -DVULKAN_HEADERS_INSTALL_DIR=/usr/local \
  -DBUILD_TESTS=OFF -DUPDATE_DEPS=OFF \
  -DBUILD_WSI_XCB_SUPPORT=ON -DBUILD_WSI_XLIB_SUPPORT=ON -DBUILD_WSI_WAYLAND_SUPPORT=ON
cmake --build loader-build --parallel "$(nproc)"
DESTDIR="$work/stage" cmake --install loader-build
package jellypilot-build-vulkan-loader 1.4.309 vulkan-loader LICENSE.txt
;;
ffmpeg)

# Enables real NVIDIA decode/encode when the user's driver supports it. The
# driver itself is never included in the AppImage, just like VA-API/VDPAU ICDs.
checkout nv-codec-headers https://github.com/FFmpeg/nv-codec-headers.git 1889e62e2d35ff7aa9baca2bceb14f053785e6f1
make -C nv-codec-headers PREFIX=/usr/local DESTDIR="$work/stage" install
mapfile -t nv_headers < <(git -C nv-codec-headers ls-files 'include/*.h')
package jellypilot-build-nv-codec-headers 12.1.14.0 nv-codec-headers "${nv_headers[@]}"

# FFmpeg 7.1.3 satisfies the pinned mpv's >=6.1 ABI requirements. Leave all
# built-in decoders/demuxers/protocols/filters enabled; add real distro codecs,
# HTTPS, SMB/SSH, subtitles, CPU scaling and hardware decoding. No nonfree code.
checkout ffmpeg https://github.com/FFmpeg/FFmpeg.git f46e514491172d15bd74b4abb1814cd2f05a763e
cd ffmpeg
./configure --prefix=/usr/local --libdir=/usr/local/lib \
  --enable-shared --disable-static --disable-debug --disable-doc \
  --enable-gpl --enable-version3 --enable-gnutls \
  --enable-libass --enable-libaom --enable-libdav1d --enable-libx264 --enable-libx265 \
  --enable-libvpx --enable-libopus --enable-libvorbis --enable-libmp3lame \
  --enable-libsoxr --enable-libopenjpeg --enable-libwebp --enable-libxml2 \
  --enable-libsmbclient --enable-libssh --enable-librubberband --enable-libzimg \
  --enable-libbluray --enable-libdrm --enable-vaapi --enable-vdpau \
  --enable-libmfx --enable-ffnvcodec --enable-nvdec --enable-nvenc --enable-vulkan
make -j"$(nproc)"
make DESTDIR="$work/stage" install
cd "$work"
package jellypilot-build-ffmpeg 7.1.3 ffmpeg COPYING.GPLv2 COPYING.GPLv3 COPYING.LGPLv2.1 COPYING.LGPLv3 LICENSE.md
cat /usr/share/doc/jellypilot-build-nv-codec-headers/copyright \
  >> /usr/share/doc/jellypilot-build-ffmpeg/copyright
;;
*) echo "Unknown build stage: $1" >&2; exit 1 ;;
esac

# Keep source identity/configuration in package copyright; intermediate objects
# are not part of the build image and must not become accidental release inputs.
rm -rf "$work"
