#!/usr/bin/env bash
set -e

# Number of missed reminders from CLI argument (default: 2)
COUNT=${1:-2}
TMP_DIR=$(mktemp -d -t rmd-test-XXXXXX)

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

# Generate JSON state file with past trigger timestamps
JSON="["
for i in $(seq 1 "$COUNT"); do
    JSON+='{"id":'"$i"',"message":"Test missed notification '"$i"'","trigger_at":1000}'
    if [ "$i" -lt "$COUNT" ]; then JSON+=','; fi
done
JSON+="]"

echo "$JSON" > "$TMP_DIR/reminders.json"

echo "=== Launching test daemon with $COUNT missed reminder(s) ==="
RMD_STATE_DIR="$TMP_DIR" RMD_SOCKET_PATH="$TMP_DIR/rmd.sock" cargo run -q -- daemon
