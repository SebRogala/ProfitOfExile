.PHONY: build test qa up down migrate migrate-down

build:
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
	@echo "migrate: not yet implemented (see POE-15)"

migrate-down:
	@echo "migrate-down: not yet implemented (see POE-15)"
