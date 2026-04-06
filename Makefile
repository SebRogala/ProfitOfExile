.PHONY: help build test qa up down migrate migrate-down migrate-force migration build-collector shell-collector logs-collector desktop-check desktop-test desktop-build desktop-deploy desktop-sync desktop-watch

help: ## Show available commands
	@grep -E '^[a-zA-Z_-]+:.*## ' Makefile | sed 's/:.*## /\t/' | awk -F '\t' '{printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

-include .env.local

build: ## Build Go server binary
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go build -o bin/server ./cmd/server

test: ## Run all Go tests with race detection
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go test -race ./...

qa: test ## Alias for test

up: ## Start dev environment (Docker Compose)
	docker compose up -d --build

down: ## Stop dev environment
	docker compose down

migrate: ## Run pending database migrations
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go run ./cmd/migrate up

migrate-down: ## Roll back last migration
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go run ./cmd/migrate down 1

migrate-force: ## Force migration version (VERSION=N)
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go run ./cmd/migrate force $(VERSION)

migration: ## Create migration files (name=add_foo_column)
ifndef name
	$(error Usage: make migration name=add_foo_column)
endif
	@ts=$$(date +%Y%m%d%H%M%S); \
	touch internal/db/migrations/$${ts}_$(name).up.sql; \
	touch internal/db/migrations/$${ts}_$(name).down.sql; \
	echo "Created: internal/db/migrations/$${ts}_$(name).{up,down}.sql"

build-collector: ## Build collector binary
	@docker compose exec collector true 2>/dev/null || $(MAKE) up
	docker compose exec collector go build -o bin/collector ./cmd/collector

shell-collector: ## Open shell in collector container
	@docker compose exec collector true 2>/dev/null || $(MAKE) up
	docker compose exec collector sh

logs-collector: ## Follow collector logs
	docker compose logs -f collector

desktop-check: ## Cargo check desktop (Rust)
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo check

desktop-test: ## Run desktop Rust tests
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo test

desktop-build: ## Build desktop release binary
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo build --release

desktop-deploy: ## Copy desktop binary to DESKTOP_DEPLOY_DIR
ifndef DESKTOP_DEPLOY_DIR
	$(error Set DESKTOP_DEPLOY_DIR in .env.local or environment)
endif
	cp desktop/src-tauri/target/release/profitofexile-desktop $(DESKTOP_DEPLOY_DIR)/

desktop-sync: ## One-time sync desktop/ to Windows (DESKTOP_WIN_DIR)
ifndef DESKTOP_WIN_DIR
	$(error Set DESKTOP_WIN_DIR in .env.local — e.g. /mnt/c/Users/you/Projects/poe-desktop)
endif
	rsync -av --delete \
		--exclude node_modules --exclude .svelte-kit --exclude build \
		--exclude target --exclude Cargo.lock \
		desktop/ $(DESKTOP_WIN_DIR)/

desktop-watch: ## Watch + sync desktop/ to Windows on changes
ifndef DESKTOP_WIN_DIR
	$(error Set DESKTOP_WIN_DIR in .env.local — e.g. /mnt/c/Users/you/Projects/poe-desktop)
endif
	@echo "Watching desktop/ → $(DESKTOP_WIN_DIR) (Ctrl+C to stop)"
	@while true; do \
		inotifywait -r -e modify,create,delete,move desktop/ \
			--exclude '(node_modules|\.svelte-kit|target|build)' 2>/dev/null; \
		rsync -av --delete \
			--exclude node_modules --exclude .svelte-kit --exclude build \
			--exclude target --exclude Cargo.lock \
			desktop/ $(DESKTOP_WIN_DIR)/; \
	done
