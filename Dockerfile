# syntax=docker/dockerfile:1

# ---- build: cross-compile a static musl binary with cargo-zigbuild ----------
# The builder is pinned to the native build platform; zig cross-compiles to the
# target arch's musl triple, so no qemu emulation is needed — an arm64 image
# builds at the host's native speed. The only C dependencies are the bundled
# SQLite and mimalloc, both compiled by zig cc; the favicon renderer (resvg) is
# pure Rust, so no CMake or system libraries are required.
# No Rust version here: rust-toolchain.toml is the single source of truth and
# rustup installs it below. Do not "simplify" this to `rust:1.97` — the
# un-suffixed tag resolves to trixie, which would be a silent Debian major bump.
FROM --platform=$BUILDPLATFORM rust:bookworm AS build

# curl + xz fetch zig; that is the only build-time system dependency.
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Zig 0.14.1 avoids the libc++-19 bindgen requirement that 0.15+ introduces.
ARG ZIG_VERSION=0.14.1
# 0.23.0 is the floor: rustc 1.98 passes `-Wl,--fix-cortex-a53-843419` as a
# pre-link arg for aarch64-unknown-linux-musl, which zig cc rejects outright.
# cargo-zigbuild filters that arg out as of rust-cross/cargo-zigbuild#452,
# first released in 0.23.0 — anything older fails the arm64 build.
ARG ZIGBUILD_VERSION=0.23.0
RUN cargo install cargo-zigbuild --version "${ZIGBUILD_VERSION}" --locked
RUN set -eux; \
    case "$(uname -m)" in \
      x86_64) zarch=x86_64 ;; \
      aarch64) zarch=aarch64 ;; \
      *) echo "unsupported build arch $(uname -m)" >&2; exit 1 ;; \
    esac; \
    curl -fsSL "https://ziglang.org/download/${ZIG_VERSION}/zig-${zarch}-linux-${ZIG_VERSION}.tar.xz" \
      | tar -xJ -C /opt; \
    ln -s "/opt/zig-${zarch}-linux-${ZIG_VERSION}/zig" /usr/local/bin/zig

WORKDIR /app

# Install the pinned toolchain in a layer keyed on rust-toolchain.toml alone, so
# editing source does not re-download the compiler. Any rustup proxy invocation
# triggers the install.
COPY rust-toolchain.toml .
RUN cargo --version

COPY . .

# Map Docker's TARGETARCH onto the Rust musl triple and build. `rustup target
# add` runs after the source (and rust-toolchain.toml) is in place, so it
# resolves against the pinned toolchain rather than the base image's default.
# .git is excluded by .dockerignore, so build.rs reads the version from the
# GIT_VERSION build arg the CI workflow passes.
ARG TARGETARCH
ARG GIT_VERSION=dev
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target,sharing=locked \
    set -eux; \
    case "$TARGETARCH" in \
      amd64) target=x86_64-unknown-linux-musl ;; \
      arm64) target=aarch64-unknown-linux-musl ;; \
      *) echo "unsupported target arch $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    rustup target add "$target"; \
    GIT_VERSION="${GIT_VERSION}" cargo zigbuild --release --target "$target"; \
    install -Dm755 "target/${target}/release/liftlog" /out/liftlog

# ---- runtime: minimal static image (CA certs + tzdata, no shell) ------------
# distroless/static (not :nonroot) keeps the root runtime user the previous
# distroless/cc image defaulted to, so the bind-mounted /data SQLite file stays
# writable without a permissions change.
FROM gcr.io/distroless/static-debian12
COPY --from=build /out/liftlog /liftlog

VOLUME /data

ENV DATABASE_URL=/data/liftlog.sqlite3
ENV LIFTLOG_BIND=0.0.0.0:8080

EXPOSE 8080

ENTRYPOINT ["/liftlog"]
