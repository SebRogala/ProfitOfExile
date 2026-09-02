.PHONY: help build test test-integration qa up down migrate migrate-down migrate-force migration build-collector shell-collector logs-collector desktop-check desktop-test desktop-test-js desktop-build desktop-deploy desktop-sync desktop-watch merc-seed-art

help: ## Show available commands
	@grep -E '^[a-zA-Z_-]+:.*## ' Makefile | sed 's/:.*## /\t/' | awk -F '\t' '{printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

-include .env.local

build: ## Build Go server binary
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go build -o bin/server ./cmd/server

test: ## Run all Go tests with race detection
	@docker compose exec app true 2>/dev/null || $(MAKE) up
	docker compose exec app go test -race ./...

test-integration: ## Run Go integration tests (build tag: integration) against a throwaway database
	./scripts/integration-test.sh

# desktop-test (cargo) is in the gate despite the earlier decision to keep it
# out for build cost: measured 37s for the whole qa run with its 119 Rust tests
# included. Accepted trade-off — the gate pays a warm cargo build so the desktop
# Rust tests run somewhere. Revisit if a cold build makes qa too slow to run.
qa: test desktop-test desktop-test-js deps-check ## Run the Go suite, the desktop Rust suite, the desktop vitest suite and the dependency-contract check

# `npm ls --all` exits non-zero when an installed package violates a declared
# peer range. That state is otherwise invisible until it misbehaves: the
# vite-plugin-svelte 4 / Vite 6 mismatch bundled Svelte's *server* runtime into
# the desktop dev build, turning every `untrack` into a no-op — tests kept
# passing, only real typing broke. Nothing else in the gate reads peer ranges.
deps-check: ## Fail if installed npm deps violate declared peer ranges
	docker compose run --rm -w /app/desktop desktop sh -c 'npm ls --all >/dev/null'
	docker compose run --rm frontend sh -c 'cd /app && npm ls --all >/dev/null'

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

# Where the merc seed-art fixture lives and which map names it. The art is
# NOT committed (GPL-3 repo, CC-BY-NC-SA icons — see that directory's README
# and .gitignore), so every machine that runs the desktop Rust suite fetches it
# first. CI does exactly that, against prod, before `cargo test`.
MERC_SEED_ART_DIR := desktop/src-tauri/tests/fixtures/merc-seed-art
MERC_SEED_MAP := desktop/src-tauri/src/mercenary/seed-map.json
# Overridable on the command line: `make merc-seed-art POE_SERVER_URL=https://profitofexile.top`.
# A local dev server fetches each icon from the wiki on first request, so the
# first local run is slow; prod already holds all 223 support icons.
POE_SERVER_URL ?= https://profitofexile.localhost

# TLS verification is skipped only for the local Traefik cert (.localhost); CI fetches from prod verified.
MERC_SEED_CURL_INSECURE := $(if $(findstring .localhost,$(POE_SERVER_URL)),-k,)
merc-seed-art: ## Fetch the merc seed gem art fixture (see its README; honours POE_SERVER_URL)
	@mkdir -p $(MERC_SEED_ART_DIR)
	@list=$$(mktemp); \
	python3 -c 'import json, sys, urllib.parse; \
	entries = json.load(open(sys.argv[1]))["entries"]; \
	slug = lambda g: "".join(c.lower() if c.isalnum() and c.isascii() else "-" for c in g); \
	print("\n".join("%s\t%s" % (slug(e["gem"]), urllib.parse.quote(e["gem"], safe="")) for e in entries))' \
		$(MERC_SEED_MAP) > "$$list" || { echo "could not read $(MERC_SEED_MAP)"; exit 1; }; \
	fetched=0; skipped=0; failed=0; \
	while IFS="$$(printf '\t')" read -r slug enc; do \
		[ -n "$$slug" ] || continue; \
		out="$(MERC_SEED_ART_DIR)/$$slug.png"; \
		if [ -s "$$out" ]; then skipped=$$((skipped+1)); continue; fi; \
		code=$$(curl -s $(MERC_SEED_CURL_INSECURE) -o "$$out" -w '%{http_code}' --max-time 30 \
			"$(POE_SERVER_URL)/api/gem-icon/$$enc" 2>/dev/null); \
		if [ "$$code" = "200" ] && [ -s "$$out" ]; then \
			fetched=$$((fetched+1)); \
		else \
			rm -f "$$out"; failed=$$((failed+1)); \
			echo "  FAILED $$slug (HTTP $${code:-no response})"; \
		fi; \
	done < "$$list"; \
	rm -f "$$list"; \
	echo "merc seed art from $(POE_SERVER_URL): $$fetched fetched, $$skipped already present, $$failed failed"; \
	[ "$$failed" -eq 0 ]

desktop-check: ## Cargo check desktop (Rust)
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo check

desktop-check-windows: ## Cargo check desktop for the Windows target (type-checks the cfg(windows) half: overlay hook, click-through, capture)
	docker compose run --rm -w /app/desktop/src-tauri desktop sh -c 'rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1; cargo check --target x86_64-pc-windows-gnu'

desktop-test: ## Run desktop Rust tests
	docker compose run --rm -w /app/desktop/src-tauri desktop cargo test

desktop-test-js: ## Run desktop JS/TS tests (vitest)
	docker compose run --rm -w /app/desktop desktop sh -c '[ -x node_modules/.bin/vitest ] || npm ci; npm test'

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
