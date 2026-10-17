#!/bin/sh
# Run the smoke or browser tests (tests/smoke or tests/e2e) against the release binary.
#
# The binary runs on this machine on a free port, on synthetic issues in target/e2e-data; the
# tests run in the Playwright image and write the issue files through a bind mount.
# Usage: tests/e2e/run.sh smoke|e2e
set -eu

suite=${1:?usage: tests/e2e/run.sh smoke|e2e}
root=$(cd "$(dirname "$0")/../.." && pwd)
binary="$root/target/release/issue-viewer"
data="$root/target/e2e-data"
log="$root/target/e2e-viewer.log"

[ -x "$binary" ] || { echo "No $binary: run make build first." >&2; exit 1; }

# Docker Desktop's file sharing keeps a removed directory behind a bind mount for a moment, and a
# container started right after it is recreated can find /data empty. So the data directory
# stays from one run to the next and only what is in it is removed.
empty_data() { find "$data" -mindepth 1 -delete; }

mkdir -p "$data"
empty_data
mkdir -p "$data/demo" "$data/archive"

# Docker Desktop reaches the host's loopback as host.docker.internal; on Linux the container
# shares the host's network instead.
if [ "$(uname -s)" = Linux ]; then
    host=127.0.0.1
    network="--network=host"
else
    host=host.docker.internal
    network=""
fi

# The data directory sits inside this repository; the ceiling keeps the board from reading this
# repository's own branches as work on the synthetic issues. The gh the viewer finds first on the
# PATH is the fake one, which answers from synthetic JSON and never reaches GitHub.
GIT_CEILING_DIRECTORIES="$data" PATH="$root/tests/fixtures/github:$PATH" GITHUB_REPOS="acme-notes/*" \
    PROJECTS="demo=$data/demo,archive=$data/archive:ro" OPEN_ROOT="$data" ALLOWED_HOSTS="$host" \
    TZ=UTC REFRESH_SECONDS=1 LOG_LEVEL=WARNING PORT=0 \
    "$binary" --env-file /dev/null >"$log" 2>&1 &
viewer=$!
trap 'kill $viewer 2>/dev/null || true; wait $viewer 2>/dev/null || true; empty_data' EXIT INT TERM

# The binary prints its address as the first line once it listens.
url=""
for _ in $(seq 1 50); do
    url=$(head -n 1 "$log" 2>/dev/null || true)
    case "$url" in http://*) break ;; esac
    kill -0 "$viewer" 2>/dev/null || { cat "$log" >&2; exit 1; }
    sleep 0.1
done
case "$url" in http://*) ;; *) echo "The viewer did not start:" >&2; cat "$log" >&2; exit 1 ;; esac
port=${url##*:}

status=0
# The tests run as this user, so what they write into the data directory belongs to the user the
# viewer runs as, and the cleanup can remove it.
# shellcheck disable=SC2086 # $network is empty or one flag
docker run --rm $network --ipc=host --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "$data:/data" -v "$root:/work:ro" -w /work \
    -e BASE_URL="http://$host:$port" -e DATA_DIR=/data -e PYTHONPATH=/work \
    -e PYTEST_ADDOPTS="-p no:cacheprovider" -e PYTHONDONTWRITEBYTECODE=1 \
    issue-viewer:e2e pytest -ra "tests/$suite" || status=$?
if [ "$status" -ne 0 ] && [ -s "$log" ]; then
    echo "--- the viewer's log ---" >&2
    cat "$log" >&2
fi
exit "$status"
