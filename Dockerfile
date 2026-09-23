# syntax=docker/dockerfile:1

# ---- build: static musl binary, cross-compiled natively by cargo-zigbuild ----
# No qemu: zig cross-compiles to the target's musl triple. No Rust version:
# rust-toolchain.toml is the source of truth. Keep the `bookworm` suffix — a
# bare `rust:<version>` tag resolves to trixie, a silent Debian major bump.
FROM --platform=$BUILDPLATFORM rust:bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl xz-utils \
    && rm -rf /var/lib/apt/lists/*

# 0.15+ needs libc++-19 for bindgen.
ARG ZIG_VERSION=0.14.1
# Floor: older releases pass rustc 1.98's `-Wl,--fix-cortex-a53-843419` to
# zig cc, which rejects it (rust-cross/cargo-zigbuild#452).
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

# Install the pinned toolchain in its own layer so source edits don't
# re-download it.
COPY rust-toolchain.toml .
RUN cargo --version

COPY . .

# .git is excluded by .dockerignore, so build.rs takes GIT_VERSION from CI.
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

# ---- runtime: distroless static (CA certs + tzdata, no shell) ---------------
# Root user (not :nonroot) so existing bind-mounted /data stays writable.
FROM gcr.io/distroless/static-debian12
COPY --from=build /out/liftlog /liftlog

VOLUME /data

ENV DATABASE_URL=/data/liftlog.sqlite3
ENV LIFTLOG_BIND=0.0.0.0:8080

EXPOSE 8080

ENTRYPOINT ["/liftlog"]
