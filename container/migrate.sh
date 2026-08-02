#!/bin/sh
set -eu

migrations_path="${MIGRATIONS_PATH:-/app/migrations}"

sqlx database create --database-url "$DATABASE_URL"
exec sqlx migrate run --database-url "$DATABASE_URL" --source "$migrations_path"
