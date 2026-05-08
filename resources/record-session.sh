#!/bin/sh
# AuthPilot session recorder - managed automatically. Do not edit.
# Reads Codex hook JSON from stdin and writes the current active session payload.

AUTHPILOT_DATA="$HOME/Library/Application Support/AuthPilot"
OUTPUT="$AUTHPILOT_DATA/codex-active-session.json"

mkdir -p "$AUTHPILOT_DATA"

INPUT=$(cat)
if [ -z "$INPUT" ]; then
    INPUT="{}"
fi

if command -v jq >/dev/null 2>&1; then
    printf '%s' "$INPUT" | jq --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        '. + {recorded_at: $ts}' > "$OUTPUT"
else
    printf '%s\n' "$INPUT" > "$OUTPUT"
fi

EVENT=$(printf '%s' "$INPUT" | grep -o '"hook_event_name":"[^"]*"' | cut -d'"' -f4)
if [ "$EVENT" = "stop" ] || [ "$EVENT" = "Stop" ]; then
    printf '%s\n' '{"__authpilot_clean_exit":true}' >> "$OUTPUT"
fi
