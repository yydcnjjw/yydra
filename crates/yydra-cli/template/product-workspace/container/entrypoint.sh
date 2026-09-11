#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
set -eu

secrets_dir=/var/lib/yydra

case "${1:-}" in
    init)
        umask 077
        for name in postgres-password cursor-signing-key; do
            path="$secrets_dir/$name"
            if [ ! -e "$path" ]; then
                # Publish complete files; never replace an existing credential.
                od -An -N32 -tx1 /dev/urandom | tr -d ' \n' > "$path.tmp"
                mv "$path.tmp" "$path"
            fi
            value=$(cat "$path")
            case "$value" in
                *[!0-9a-f]*|'') echo "Invalid saved credential: $name" >&2; exit 1 ;;
            esac
            if [ "${#value}" -ne 64 ]; then
                echo "Invalid saved credential length: $name" >&2
                exit 1
            fi
        done
        exit 0
        ;;
    server|migrate)
        if [ -z "${DATABASE_URL:-}" ]; then
            password=$(cat "$secrets_dir/postgres-password")
            DATABASE_URL="postgres://postgres:$password@postgres:5432/yydra_product"
            export DATABASE_URL
        fi
        if [ "$1" = server ] && [ -z "${YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY:-}" ]; then
            YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY=$(cat "$secrets_dir/cursor-signing-key")
            export YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY
        fi
        ;;
esac

exec "$@"
