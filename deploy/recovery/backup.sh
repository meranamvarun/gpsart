#!/bin/sh
set -eu
umask 077

: "${PGHOST:?PGHOST is required}"
: "${PGDATABASE:?PGDATABASE is required}"
: "${PGUSER:?PGUSER is required}"
: "${PGPASSWORD:?PGPASSWORD is required}"
: "${RESTIC_REPOSITORY:?RESTIC_REPOSITORY is required}"
: "${RESTIC_PASSWORD:?RESTIC_PASSWORD is required}"
: "${GPSART_BACKUP_RETENTION_DAYS:?GPSART_BACKUP_RETENTION_DAYS is required}"

case "$GPSART_BACKUP_RETENTION_DAYS" in
  ''|*[!0-9]*) exit 2 ;;
esac
if [ "$GPSART_BACKUP_RETENTION_DAYS" -lt 1 ] || [ "$GPSART_BACKUP_RETENTION_DAYS" -gt 7 ]; then
  exit 2
fi

if ! restic snapshots --tag gpsart-production >/dev/null 2>&1; then
  restic init >/dev/null
fi
pg_dump --format=custom --no-owner --no-acl | restic backup --stdin --stdin-filename gpsart.dump --tag gpsart-production --tag automated
restic forget --keep-within "${GPSART_BACKUP_RETENTION_DAYS}d" --prune --tag gpsart-production
