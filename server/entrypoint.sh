#!/bin/sh
# Starts the newest Zima Connect server: the one in the image, or the one the dashboard
# downloaded into /data/bin ("Update and restart"). An update exits with code 42.
IMG="${ZC_IMAGE_BIN:-/usr/local/bin/zima-connect-hub}"
UPD="${ZC_DATA_DIR:-/data}/bin/zima-connect-hub"
export ZC_SELF_UPDATE=1

newer() { # is version $1 newer than $2?
  [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" = "$1" ]
}

child=0
trap 'kill -TERM "$child" 2>/dev/null' TERM INT

while true; do
  RUN="$IMG"
  if [ -x "$UPD" ]; then
    iv=$("$IMG" --version 2>/dev/null || echo 0)
    uv=$("$UPD" --version 2>/dev/null || echo 0)
    if newer "$uv" "$iv"; then RUN="$UPD"; else rm -f "$UPD"; fi # a newer image wins
  fi
  "$RUN" "$@" &
  child=$!
  wait "$child"
  code=$?
  if kill -0 "$child" 2>/dev/null; then # a signal ended the first wait early
    wait "$child"
    code=$?
  fi
  [ "$code" -eq 42 ] || exit "$code"
done
