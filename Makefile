.PHONY: build run test up down migrate migrate-down

build:
	go build -o bin/server ./cmd/server

run:
	go run ./cmd/server

test:
	go test -race ./...

up:
	docker compose up -d

down:
	docker compose down

migrate:
	@echo "migrate: not yet implemented (see POE-15)"

migrate-down:
	@echo "migrate-down: not yet implemented (see POE-15)"
