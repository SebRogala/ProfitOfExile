# Stage 0: Build frontend (SvelteKit with adapter-static)
FROM node:22-alpine AS frontend-build

WORKDIR /app

COPY frontend/package*.json ./
RUN npm ci

COPY frontend/ .
RUN npm run build

# Stage 1: Build the Go binary
FROM golang:1.23 AS build

WORKDIR /src

COPY go.mod go.sum ./
RUN go mod download

# NOTE: COPY . . brings frontend/ source into this stage even though it is not
# needed (only the build output from stage 0 matters). .dockerignore cannot
# exclude frontend/ per-stage without breaking the frontend-build stage above.
# Acceptable for now; Go module cache is already preserved by the earlier
# go.mod/go.sum layer.
COPY . .
# Place SvelteKit build output where the Go embed directive expects it.
COPY --from=frontend-build /app/build ./cmd/server/frontend_build

ARG GIT_SHA=dev
RUN CGO_ENABLED=0 go build -ldflags="-s -w -X profitofexile/internal/server/handlers.Version=${GIT_SHA}" -o /bin/server ./cmd/server
RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o /bin/collector ./cmd/collector
RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o /bin/recalculate ./cmd/recalculate
RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o /bin/promote ./cmd/promote

# Stage 2: Minimal production image (collector)
# NOTE: build with --target to select image:
#   docker build --target server   -t profitofexile-server .
#   docker build --target collector -t profitofexile-collector .
FROM gcr.io/distroless/static-debian12 AS collector

COPY --from=build /bin/collector /collector

EXPOSE 8090

ENTRYPOINT ["/collector"]

# Stage 3: Minimal production image (server) — default target
FROM gcr.io/distroless/static-debian12 AS server

COPY --from=build /bin/server /server
COPY --from=build /bin/recalculate /recalculate
COPY --from=build /bin/promote /promote

EXPOSE 8080

ENTRYPOINT ["/server"]
