# syntax=docker/dockerfile:1.18@sha256:dabfc0969b935b2080555ace70ee69a5261af8a8f1b4df97b9e7fbcf6722eddf
FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release --locked -p gpsart-persistence --bin gpsart-migrate --bin gpsart-recovery-audit

FROM restic/restic@sha256:b43f00a24d9b2d9affb332a29d57042563eeb61219ae1a36dfccbdc34fbaa6b3 AS restic

FROM postgis/postgis:16-3.4@sha256:44126d872ac91993766c341e369c539e8196614321765d36a6f1bab0419a5fa5
COPY --from=restic /usr/bin/restic /usr/local/bin/restic
COPY --from=build /src/target/release/gpsart-migrate /usr/local/bin/gpsart-migrate
COPY --from=build /src/target/release/gpsart-recovery-audit /usr/local/bin/gpsart-recovery-audit
COPY --chmod=0555 deploy/recovery/backup.sh /usr/local/bin/gpsart-backup
COPY --chmod=0555 deploy/recovery/rehearse.sh /usr/local/bin/gpsart-rehearse
USER 999:999
ENTRYPOINT ["/usr/local/bin/gpsart-backup"]
