FROM docker.io/library/ubuntu:22.04

# Runtime/display tools only: never inherit the build image's development packages.
# Mesa is the distribution's normal Vulkan implementation, not a forced renderer
# or ICD override. No host GPU, display, session, credentials or config is mounted.
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        dbus-x11 fonts-dejavu-core libegl1 libgl1-mesa-dri libvulkan1 \
        libxkbcommon0 mesa-vulkan-drivers xauth xdotool xvfb \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --no-create-home --home-dir /tmp/home smoke

COPY packaging/linux/appimage-smoke.sh /usr/local/libexec/appimage-smoke.sh
USER 1000:1000
WORKDIR /tmp
ENTRYPOINT ["dbus-run-session", "--", "xvfb-run", "-a", "-s", "-screen 0 800x600x24 -nolisten tcp", "bash", "/usr/local/libexec/appimage-smoke.sh"]
