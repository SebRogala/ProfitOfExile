# Stage 1: Build the Go binary
FROM golang:1.23 AS build

WORKDIR /src

COPY go.mod go.sum ./
RUN go mod download

COPY . .

RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o /bin/server ./cmd/server

# Stage 2: Minimal production image
FROM gcr.io/distroless/static-debian12

COPY --from=build /bin/server /server

EXPOSE 8080

ENTRYPOINT ["/server"]
