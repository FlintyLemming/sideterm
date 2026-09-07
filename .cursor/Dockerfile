# Cloud Agent base image for SideTerm (a WezTerm fork).
#
# System packages and stable toolchains live here; repository-dependent work
# (git submodules + `cargo build`) runs in the `install` phase defined in
# .cursor/environment.json so it can see the checked-out source.
#
# The apt package list mirrors the Debian/Ubuntu branch of the repo's own
# `get-deps` script (plus a few build essentials). Keep them in sync if
# `get-deps` changes.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Core build tooling plus the X11/Wayland/font/OpenSSL dev libraries WezTerm
# needs to build and link its GUI. openssh-server is required by the
# wezterm-ssh end-to-end tests.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bsdutils \
        build-essential \
        ca-certificates \
        cmake \
        curl \
        dpkg-dev \
        fakeroot \
        g++ \
        gcc \
        git \
        libegl1-mesa-dev \
        libfontconfig1-dev \
        libssl-dev \
        libwayland-dev \
        libx11-xcb-dev \
        libxcb-ewmh-dev \
        libxcb-icccm4-dev \
        libxcb-image0-dev \
        libxcb-keysyms1-dev \
        libxcb-randr0-dev \
        libxcb-render0-dev \
        libxcb-util-dev \
        libxcb-xkb-dev \
        libxkbcommon-dev \
        libxkbcommon-x11-dev \
        locales \
        lsb-release \
        openssh-client \
        openssh-server \
        pkg-config \
        python3 \
        sudo \
        xdg-utils \
        xorg-dev \
    && rm -rf /var/lib/apt/lists/*

# The default cloud-agent user. Created with passwordless sudo so the agent can
# install extra packages on demand.
RUN if ! id ubuntu >/dev/null 2>&1; then \
        useradd --create-home --shell /bin/bash --uid 1000 ubuntu; \
    fi \
    && echo 'ubuntu ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/ubuntu \
    && chmod 0440 /etc/sudoers.d/ubuntu

# System-wide Rust toolchain (stable). SideTerm's dependencies require an
# edition-2024-capable toolchain, so we track the latest stable release rather
# than the older rustc that some base images ship.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --default-toolchain stable --profile minimal --component rustfmt clippy \
    && curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /usr/local/cargo/bin \
    && chmod -R a+rwX "$RUSTUP_HOME" "$CARGO_HOME"

USER ubuntu
