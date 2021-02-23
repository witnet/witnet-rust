#!/usr/bin/env bash

WITNET_BINARY=${1:-./witnet}
WITNET_CONFIG_FILE=${2:-./witnet.toml}

RECOVERY_BANNER="

███████╗ ██████╗ ██████╗ ██╗  ██╗
██╔════╝██╔═══██╗██╔══██╗██║ ██╔╝
█████╗  ██║   ██║██████╔╝█████╔╝
██╔══╝  ██║   ██║██╔══██╗██╔═██╗
██║     ╚██████╔╝██║  ██║██║  ██╗
╚═╝      ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝
██████╗ ███████╗ ██████╗ ██████╗
██╔══██╗██╔════╝██╔════╝██╔═══██╗
██████╔╝█████╗  ██║     ██║   ██║
██╔══██╗██╔══╝  ██║     ██║   ██║
██║  ██║███████╗╚██████╗╚██████╔╝
╚═╝  ╚═╝╚══════╝ ╚═════╝ ╚═════╝
██╗   ██╗███████╗██████╗ ██╗   ██╗
██║   ██║██╔════╝██╔══██╗╚██╗ ██╔╝
██║   ██║█████╗  ██████╔╝ ╚████╔╝
╚██╗ ██╔╝██╔══╝  ██╔══██╗  ╚██╔╝
 ╚████╔╝ ███████╗██║  ██║   ██║
  ╚═══╝  ╚══════╝╚═╝  ╚═╝   ╚═╝

╔══════════════════════════════════════════════════════════╗
║ LOCAL CHAIN IS FORKED. PROCEEDING TO AUTOMATIC RECOVERY. ║
╠══════════════════════════════════════════════════════════╣
║ This process will sanitize the local chain state by      ║
║ rewinding back to the point where the fork took place,   ║
║ and then continue synchronizing and operating as usual.  ║
╟──────────────────────────────────────────────────────────╢
║ This will take from 30 to 60 minutes depending on your   ║
║ network, CPU, RAM and hard disk speeds.                  ║
╟──────────────────────────────────────────────────────────╢
║ Learn more about why this recovery is needed:            ║
║ https://github.com/witnet/WIPs/blob/master/wip-0010.md   ║
╟──────────────────────────────────────────────────────────╢
║ Feel free to ask any questions on:                       ║
║ Discord:  https://discord.gg/X4uurfP                     ║
║ Telegram: https://t.me/witnetio                          ║
╚══════════════════════════════════════════════════════════╝
"

# Just a pretty logging helper
function log {
  echo "[WIP0010_RECOVERY] $1"
}

# This script can be skipped by setting environment variable SKIP_WIP0010_RECOVERY to "true"
if [[ "$SKIP_WIP0010_RECOVERY" == "true" ]]; then
  log "Skipping WIP-0010 recovery"
  exit 0
fi

# Make sure the arguments make sense
if ! command -v "$WITNET_BINARY" &> /dev/null; then
  log "ERROR: The provided witnet binary (first argument to this script) is not a valid executable file: $WITNET_BINARY"
  exit 1
fi
if [ ! -f "$WITNET_CONFIG_FILE" ]; then
  log "ERROR: The provided witnet configuration file (second argument to this script) is not a valid configuration file: $WITNET_CONFIG_FILE"
  exit 2
fi

# Read configuration (e.g. node server address) from config file
log "Using configuration file at $WITNET_CONFIG_FILE"
HOST=$(grep "server_addr" "$WITNET_CONFIG_FILE" | sed -En "s/server_addr = \"(.*)\"/\1/p" | sed -En "s/0.0.0.0/127.0.0.1/p" )
ADDRESS=$(echo "$HOST" | cut -d':' -f1)
PORT=$(echo "$HOST" | cut -d':' -f2)

# Check connection to local witnet node
TIME_TO_NEXT_RETRY=5
log "Checking connection to local witnet node at $HOST"
while true
  if nc -zv "$ADDRESS" "$PORT" &>/dev/null; then
    log "Successful connection to local witnet node at $HOST"
    break
  else
    log "ERROR: Failed to connect to local witnet node at $HOST"
    log "Retrying in $TIME_TO_NEXT_RETRY seconds"
    sleep "$TIME_TO_NEXT_RETRY"
    TIME_TO_NEXT_RETRY=$(( 2 * TIME_TO_NEXT_RETRY ))
  fi
do true; done

# Check whether the local witnet node is below WIP-0010 "common checkpoint" (#248839)
if [[ "$($WITNET_BINARY node blockchain --epoch 248839 --limit 1 2>&1 | wc -l)" == "5" ]]; then
  log "The local witnet node at $HOST seems to be syncing blocks prior to the WIP-0010 fork. No recovery action is needed."
  exit 0
fi

# Check whether the local witnet node is on the `A` chain, and if so, skip recovery
if $WITNET_BINARY node blockchain --epoch 248839 --limit 2 2>&1 | grep -q '#248921 had digest 7556670d'; then
  log "The local witnet node at $HOST seems to be on the leading chain. No recovery action is needed"
  exit 0
fi

echo "$RECOVERY_BANNER"

# TODO: replace known_peers in witnet.toml
# TODO: clear peers and buckets
# TODO: rewind