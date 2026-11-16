# syntax=docker/dockerfile:1

# ---- build: cross-compile a static Go binary --------------------------------
# Runs on the native build platform; Go cross-compiles to the target with
# CGO off, so no qemu and no C toolchain.
# No Go version tag: the image's Go is used when it is at least go.mod's
# `toolchain` line; otherwise GOTOOLCHAIN=auto fetches that line (auto only
# moves up). Keep the `bookworm` suffix — a bare tag
# resolves to trixie, a silent Debian bump.
FROM --platform=$BUILDPLATFORM golang:bookworm AS build

WORKDIR /app

# The official image sets GOTOOLCHAIN=local, which ignores go.mod's toolchain
# line. Fetch a newer toolchain in a layer keyed on go.mod/go.sum alone, so
# source edits do not re-download it.
ENV GOTOOLCHAIN=auto
COPY go.mod go.sum ./
RUN go version

COPY . .

# `vendor/` ships in the repository, so the build needs no network. .git is
# excluded by .dockerignore, so the version comes from GIT_VERSION.
ARG TARGETOS
ARG TARGETARCH
ARG GIT_VERSION=dev
RUN --mount=type=cache,target=/root/.cache/go-build \
    CGO_ENABLED=0 GOOS="$TARGETOS" GOARCH="$TARGETARCH" \
    go build -mod=vendor -trimpath \
      -ldflags "-s -w -X github.com/henry40408/liftlog/internal/version.version=${GIT_VERSION}" \
      -o /out/liftlog ./cmd/liftlog

# ---- runtime: minimal static image (CA certs + tzdata, no shell) ------------
# Root user (not :nonroot) so existing bind-mounted /data stays writable.
FROM gcr.io/distroless/static-debian12
COPY --from=build /out/liftlog /liftlog

VOLUME /data

ENV DATABASE_URL=/data/liftlog.sqlite3
ENV LIFTLOG_BIND=0.0.0.0:8080

EXPOSE 8080

ENTRYPOINT ["/liftlog"]
