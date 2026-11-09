# The Linux test machine: spec §2.1's oldest glibc and x64 target, software Vulkan, X11 and
# Wayland servers without a display, and the accessibility bus. script/linux drives it.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates clang cmake curl git pkg-config rsync \
    libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev \
    libvulkan-dev libfontconfig-dev libfreetype-dev libasound2-dev libzstd-dev \
    mesa-vulkan-drivers vulkan-tools xvfb xdotool x11-utils x11-apps imagemagick \
    sway xwayland grim wtype \
    dbus dbus-x11 at-spi2-core python3-gi gir1.2-atspi-2.0 xdg-utils \
    fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*

RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
        --component clippy --component rustfmt \
    && curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /opt/cargo/bin

# The AppImage tools, pinned by digest. FUSE is missing in a container, so appimagetool is
# unpacked once here.
ARG APPIMAGETOOL=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
ARG APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
ARG RUNTIME=https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
ARG RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
RUN apt-get update && apt-get install -y --no-install-recommends file patchelf desktop-file-utils \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /opt/appimage && cd /opt/appimage \
    && curl -LsSf -o appimagetool "$APPIMAGETOOL" \
    && curl -LsSf -o runtime "$RUNTIME" \
    && echo "$APPIMAGETOOL_SHA256  appimagetool" | sha256sum -c \
    && echo "$RUNTIME_SHA256  runtime" | sha256sum -c \
    && chmod +x appimagetool && ./appimagetool --appimage-extract > /dev/null \
    && rm appimagetool

# Root ignores file permissions, and the tests of unreadable and read-only paths need them.
RUN useradd --create-home triad \
    && mkdir -p /work /target /opt/cargo/registry \
    && chown -R triad: /work /target /opt/cargo /opt/rustup
USER triad
