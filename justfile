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

# Start the application server used by Playwright
e2e-server:
    LITE_VOTE_DATABASE_PATH="var/e2e.sqlite3" just dev

# Install Playwright's test dependency and three browser engines
e2e-install:
    npm ci
    npx playwright install chromium firefox webkit

# Run the smoke test in Chromium
e2e:
    npm run test:e2e

# Run the smoke test in Chromium, Firefox, and WebKit
e2e-all:
    npm run test:e2e:all

# Build the production OCI image
container-build:
    docker build --target runtime --tag lite-vote:local .

# Build and start the local container stack
container-up:
    docker compose up --build

# Stop the local container stack while retaining SQLite data
container-down:
    docker compose down

# Create a new SQLx migration with the given name
db-add name:
    sqlx migrate add {{ quote(name) }}

# Run formatting, linting, and tests
check:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features
