ARG CROSS_BASE_IMAGE
# FROM $CROSS_BASE_IMAGE
FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main

ARG CROSS_DEB_ARCH

RUN dpkg --add-architecture $CROSS_DEB_ARCH
RUN apt-get update && apt-get --assume-yes install libdbus-1-dev:$CROSS_DEB_ARCH pkg-config:$CROSS_DEB_ARCH
# RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain none -y
# ENV PATH="/root/.cargo/bin:${PATH}"
# RUN rustup toolchain install nightly --allow-downgrade --profile minimal --component rust-analyzer
# RUN rust-analyzer --help
