.PHONY: build test qa up down migrate migrate-down migrate-force

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
