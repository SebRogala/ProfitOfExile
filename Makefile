.PHONY: build test qa up down migrate migrate-down migrate-force migration build-collector shell-collector logs-collector desktop-check desktop-test desktop-build desktop-deploy desktop-sync desktop-watch

-include .env.local

build:
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go build -o bin/server ./cmd/server

test:
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go test -race ./...

qa: test

up:
	docker compose up -d --build

down:
	docker compose down

migrate:
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go run ./cmd/migrate up

migrate-down:
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go run ./cmd/migrate down 1

migrate-force:
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go run ./cmd/migrate force $(VERSION)

migration:
ifndef name
	$(error Usage: make migration name=add_foo_column)
endif
	@ts=$$(date +%Y%m%d%H%M%S); \
	touch internal/db/migrations/$${ts}_$(name).up.sql; \
	touch internal/db/migrations/$${ts}_$(name).down.sql; \
	echo "Created: internal/db/migrations/$${ts}_$(name).{up,down}.sql"

build-collector:
	@docker compose exec collector true 2>/dev/null || $(MAKE) up
	docker compose exec collector go build -o bin/collector ./cmd/collector

shell-collector:
	@docker compose exec collector true 2>/dev/null || $(MAKE) up
	docker compose exec collector sh

logs-collector:
	docker compose logs -f collector

desktop-check:
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo check

desktop-test:
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo test

desktop-build:
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo build --release

desktop-deploy:
ifndef DESKTOP_DEPLOY_DIR
	$(error Set DESKTOP_DEPLOY_DIR in .env.local or environment)
endif
	cp desktop/src-tauri/target/release/profitofexile-desktop $(DESKTOP_DEPLOY_DIR)/

desktop-sync:
ifndef DESKTOP_WIN_DIR
	$(error Set DESKTOP_WIN_DIR in .env.local — e.g. /mnt/c/Users/you/Projects/poe-desktop)
endif
	rsync -av --delete \
		--exclude node_modules --exclude .svelte-kit --exclude build \
		--exclude target --exclude Cargo.lock \
		desktop/ $(DESKTOP_WIN_DIR)/

desktop-watch:
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
