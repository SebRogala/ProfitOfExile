#!/bin/sh
set -eu

source_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/poe-fetch-gem-art.XXXXXX)
server_pid=
cleanup() {
	if [ -n "$server_pid" ]; then
		kill "$server_pid" 2>/dev/null || :
		wait "$server_pid" 2>/dev/null || :
	fi
	rm -rf -- "$scratch"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
fail=0

python3 - "$scratch" >"$scratch/server-output" 2>&1 <<'PY' &
import base64
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
import sys

scratch = Path(sys.argv[1])
png = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
    "+A8AAQUBAScY42YAAAAASUVORK5CYII="
)
sequences = {
    "/api/icon/gems/Vaal%20Rain%20of%20Arrows": [502, 502, 200],
    "/api/icon/gems/Missing%20Gem": [404],
}
counts = {}


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        with (scratch / "requests").open("a") as log:
            log.write(self.path + "\n")
        index = counts.get(self.path, 0)
        counts[self.path] = index + 1
        sequence = sequences.get(self.path, [400])
        status = sequence[min(index, len(sequence) - 1)]
        body = png if status == 200 else b"upstream error"
        self.send_response(status)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


server = HTTPServer(("127.0.0.1", 0), Handler)
(scratch / "port").write_text(str(server.server_port))
server.serve_forever()
PY
server_pid=$!

poll=0
while [ ! -s "$scratch/port" ]; do
	if ! kill -0 "$server_pid" 2>/dev/null || [ "$poll" -ge 100 ]; then
		echo 'FAIL - HTTP stub did not become ready'
		cat "$scratch/server-output"
		exit 1
	fi
	poll=$((poll + 1))
	sleep 0.05
done
port=$(cat "$scratch/port")
: > "$scratch/requests"
mkdir -p "$scratch/art"

run_fetch() {
	status=0
	FETCH_GEM_ART_BACKOFF_BASE=0 POE_SERVER_URL="http://127.0.0.1:$port" \
		MERC_SEED_CURL_INSECURE= sh "$source_root/scripts/fetch-gem-art.sh" \
		'test gem art' "$scratch/art" < "$scratch/list" > "$scratch/output" 2>&1 || status=$?
}
request_count() {
	awk -v path="$1" '$0 == path { n++ } END { print n+0 }' "$scratch/requests"
}

name='keep:regression transient 502 recovers on the third request'
printf 'vaal-rain-of-arrows\tVaal%%20Rain%%20of%%20Arrows\n' > "$scratch/list"
run_fetch
requests=$(request_count '/api/icon/gems/Vaal%20Rain%20of%20Arrows')
if [ "$status" -eq 0 ] && [ -s "$scratch/art/vaal-rain-of-arrows.png" ] && [ "$requests" -eq 3 ]; then
	echo "ok   - $name"
else
	echo "FAIL - $name (exit $status; requests $requests, expected 3; nonempty PNG required)"
	cat "$scratch/output"
	fail=1
fi

# A retained error body would silently become trusted art for fixture consumers.
name='keep:critical 404 rejects invalid fixture art immediately'
printf 'missing-gem\tMissing%%20Gem\n' > "$scratch/list"
run_fetch
requests=$(request_count '/api/icon/gems/Missing%20Gem')
if [ "$status" -ne 0 ] && [ "$requests" -eq 1 ] && [ ! -e "$scratch/art/missing-gem.png" ] \
	&& grep -qxF '  FAILED missing-gem (HTTP 404)' "$scratch/output"; then
	echo "ok   - $name"
else
	echo "FAIL - $name (exit $status; requests $requests, expected 1; no PNG and FAILED missing-gem (HTTP 404) required)"
	cat "$scratch/output"
	fail=1
fi

exit "$fail"
