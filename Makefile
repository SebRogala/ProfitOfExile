.PHONY: build run test up down migrate migrate-down qa

GO_RUN = docker run --rm -v $(CURDIR):/app -w /app golang:1.23

build:
	$(GO_RUN) go build -o bin/server ./cmd/server

run:
	$(GO_RUN) go run ./cmd/server

test:
	$(GO_RUN) go test -race ./...

qa: test

up:
	docker compose up -d

down:
	docker compose down

migrate:
	@echo "migrate: not yet implemented (see POE-15)"

migrate-down:
	@echo "migrate-down: not yet implemented (see POE-15)"
