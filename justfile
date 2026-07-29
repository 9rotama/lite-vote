export LITE_VOTE_DATABASE_PATH := env_var_or_default("LITE_VOTE_DATABASE_PATH", "var/lite-vote.sqlite3")

[private]
default:
    @just --list

# Apply SQLx migrations to the development database
db-migrate:
    mkdir -p "$(dirname "$LITE_VOTE_DATABASE_PATH")"
    sqlx database create --database-url "sqlite://$LITE_VOTE_DATABASE_PATH"
    sqlx migrate run --database-url "sqlite://$LITE_VOTE_DATABASE_PATH"

# Start the development server after applying database migrations
dev: db-migrate
    topcoat dev

# Create a new SQLx migration with the given name
db-add name:
    sqlx migrate add {{ quote(name) }}

# Run formatting, linting, and tests
check:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features
