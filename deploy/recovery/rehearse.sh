#!/bin/sh
set -eu
umask 077

: "${PGHOST:?isolated PGHOST is required}"
: "${PGDATABASE:?PGDATABASE is required}"
: "${PGUSER:?PGUSER is required}"
: "${PGPASSWORD:?PGPASSWORD is required}"
: "${RESTIC_REPOSITORY:?RESTIC_REPOSITORY is required}"
: "${RESTIC_PASSWORD:?RESTIC_PASSWORD is required}"

restore_dir="$(mktemp -d)"
trap 'rm -rf -- "$restore_dir"' EXIT HUP INT TERM
backup_age_seconds="$(gpsart-recovery-audit --repository-age)"
restic restore latest --tag gpsart-production --target "$restore_dir" --include gpsart.dump
dump_path="$(find "$restore_dir" -name gpsart.dump -type f -print -quit)"
test -n "$dump_path"
pg_restore --exit-on-error --clean --if-exists --no-owner --no-acl --dbname "$PGDATABASE" "$dump_path"
gpsart-migrate
gpsart-recovery-audit
printf 'backup_age_seconds=%s\n' "$backup_age_seconds"
