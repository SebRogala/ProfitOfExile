#!/bin/sh
set -eu

label=$1
out_dir=$2
: "${POE_SERVER_URL:?POE_SERVER_URL is required}"
backoff_base=${FETCH_GEM_ART_BACKOFF_BASE:-2}
tab=$(printf '\t')
fetched=0
skipped=0
failed=0

while IFS= read -r line || [ -n "$line" ]; do
	slug=${line%%"$tab"*}
	enc=${line#*"$tab"}
	[ -n "$slug" ] || continue
	out="$out_dir/$slug.png"
	if [ -s "$out" ]; then
		skipped=$((skipped + 1))
		continue
	fi

	retries=0
	wait_seconds=$backoff_base
	while :; do
		curl_status=0
		code=$(curl -s ${MERC_SEED_CURL_INSECURE:-} -o "$out" -w '%{http_code}' --max-time 30 \
			"$POE_SERVER_URL/api/icon/gems/$enc" 2>/dev/null) || curl_status=$?
		if [ "$curl_status" -eq 0 ] && [ "$code" = 200 ] && [ -s "$out" ]; then
			fetched=$((fetched + 1))
			break
		fi

		case "$code" in
			5[0-9][0-9]|000|'')
				if [ "$retries" -lt 3 ]; then
					sleep "$wait_seconds"
					wait_seconds=$((wait_seconds * 2))
					retries=$((retries + 1))
					continue
				fi
				;;
		esac
		rm -f "$out"
		failed=$((failed + 1))
		echo "  FAILED $slug (HTTP ${code:-no response})"
		break
	done
done

echo "$label from $POE_SERVER_URL: $fetched fetched, $skipped already present, $failed failed"
[ "$failed" -eq 0 ]
