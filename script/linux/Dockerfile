# The Linux test machine: spec §2.1's oldest glibc on x64 or ARM64, software Vulkan, X11 and
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

# The AppImage tools of the machine's architecture, pinned by digest. FUSE is missing in a
# container, so appimagetool is unpacked once here.
ARG APPIMAGETOOL=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool
ARG APPIMAGETOOL_SHA256_X86_64=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
ARG APPIMAGETOOL_SHA256_AARCH64=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
ARG RUNTIME=https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime
ARG RUNTIME_SHA256_X86_64=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
ARG RUNTIME_SHA256_AARCH64=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
RUN apt-get update && apt-get install -y --no-install-recommends file patchelf desktop-file-utils \
    && rm -rf /var/lib/apt/lists/* \
    && arch=$(uname -m) \
    && case $arch in \
        x86_64) tool_sha=$APPIMAGETOOL_SHA256_X86_64 runtime_sha=$RUNTIME_SHA256_X86_64 ;; \
        aarch64) tool_sha=$APPIMAGETOOL_SHA256_AARCH64 runtime_sha=$RUNTIME_SHA256_AARCH64 ;; \
        *) echo "no AppImage tools for $arch" >&2; exit 1 ;; \
       esac \
    && mkdir -p /opt/appimage && cd /opt/appimage \
    && curl -LsSf -o appimagetool "$APPIMAGETOOL-$arch.AppImage" \
    && curl -LsSf -o runtime "$RUNTIME-$arch" \
    && echo "$tool_sha  appimagetool" | sha256sum -c \
    && echo "$runtime_sha  runtime" | sha256sum -c \
    && chmod +x appimagetool && ./appimagetool --appimage-extract > /dev/null \
    && rm appimagetool

# The digest of the stable channel's manifest, from script/linux/run image. A new stable
# Rust changes it, which rebuilds this layer and the ones after it.
ARG RUST_STABLE
RUN echo "stable manifest $RUST_STABLE" \
    && curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
        --component clippy --component rustfmt \
    && case $(uname -m) in aarch64) nextest=linux-arm ;; *) nextest=linux ;; esac \
    && curl -LsSf "https://get.nexte.st/latest/$nextest" | tar zxf - -C /opt/cargo/bin

# Root ignores file permissions, and the tests of unreadable and read-only paths need them.
RUN useradd --create-home triad \
    && mkdir -p /work /target /opt/cargo/registry \
    && chown -R triad: /work /target /opt/cargo /opt/rustup
USER triad
