#!/usr/bin/env bash

WITNET_BINARY=${1:-./witnet}
WITNET_CONFIG_FILE=${2:-./witnet.toml}

RECOVERY_BANNER="

███████╗ ██████╗ ██████╗ ██╗  ██╗  ██╗ ██╗ ██╗
██╔════╝██╔═══██╗██╔══██╗██║ ██╔╝  ██║ ██║ ██║
█████╗  ██║   ██║██████╔╝█████╔╝   ██║ ██║ ██║
██╔══╝  ██║   ██║██╔══██╗██╔═██╗   ██║ ██║ ██║
██║     ╚██████╔╝██║  ██║██║  ██╗  ██║ ██║ ██║
╚═╝      ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝  ╚████████╔╝
██████╗ ███████╗ ██████╗ ██████╗     ╚█████╔╝
██╔══██╗██╔════╝██╔════╝██╔═══██╗     ╚██╔╝
██████╔╝█████╗  ██║     ██║   ██║      ██║
██╔══██╗██╔══╝  ██║     ██║   ██║      ██║
██║  ██║███████╗╚██████╗╚██████╔╝      ██║
╚═╝  ╚═╝╚══════╝ ╚═════╝ ╚═════╝       ██║
██╗   ██╗███████╗██████╗ ██╗   ██╗     ██║
██║   ██║██╔════╝██╔══██╗╚██╗ ██╔╝     ██║
██║   ██║█████╗  ██████╔╝ ╚████╔╝      ██║
╚██╗ ██╔╝██╔══╝  ██╔══██╗  ╚██╔╝       ██║
 ╚████╔╝ ███████╗██║  ██║   ██║        ██║
  ╚═══╝  ╚══════╝╚═╝  ╚═╝   ╚═╝        ╚═╝

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
║ https://github.com/witnet/WIPs/blob/master/20211008.md   ║
╟──────────────────────────────────────────────────────────╢
║ Feel free to ask any questions on:                       ║
║ Discord:  https://discord.gg/X4uurfP                     ║
║ Telegram: https://t.me/witnetio                          ║
╚══════════════════════════════════════════════════════════╝
"

KNOWN_PEERS='\n    "45.43.29.114:21337",\n    "45.43.30.194:21337",\n    "45.154.212.50:21337",\n    "51.91.8.100:21301",\n    "51.91.11.234:21338",\n    "51.91.11.234:21339",\n    "51.91.11.234:21340",\n    "51.255.51.101:21337",\n    "51.255.51.101:21338",\n    "51.255.51.101:21339",\n    "52.166.178.145:21337",\n    "52.166.178.145:22337",\n    "78.20.135.129:21337",\n    "82.213.207.211:21337",\n    "104.218.233.82:21337",\n    "104.218.233.114:21337",\n    "136.243.19.123:22360",\n    "157.90.131.55:21337",\n    "159.69.74.123:22344",\n    "161.97.112.213:21337",\n    "173.249.3.178:21337",\n    "173.249.3.178:22337",\n    "173.249.3.178:41337",\n    "173.249.8.65:20337",\n    "173.249.8.65:21337",\n    "173.249.8.65:41337",\n'

# Just a pretty logging helper
function log {
  echo "[20211008_RECOVERY] $1"
}

# A helper for calculating ETAs
function eta {
  START=$1
  NOW=$2
  PROGRESS=$3
  if [ "$PROGRESS" == "00" ]; then
    echo "will be shown as synchronization moves forward..."
  else
    ELAPSED=$(( NOW - START ))
    SPEED=$((PROGRESS * 1000 / ELAPSED))
    if [ "$SPEED" == "0" ]; then
        SPEED=1
    fi
    REMAINING_PROGRESS=$(( 10000 - PROGRESS ))
    REMAINING_TIME=$((REMAINING_PROGRESS * 1000 / SPEED + 30 ))
    echo $(( REMAINING_TIME / 60 )) minutes $((REMAINING_TIME % 60)) seconds aprox.
  fi
}

# This script can be skipped by setting environment variable SKIP_20211008_RECOVERY to "true"
if [[ "$SKIP_WIP20211008_RECOVERY" == "true" ]]; then
  log "Skipping 20211008 recovery"
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
HOST=$(grep "server_addr" "$WITNET_CONFIG_FILE" | sed -En "s/server_addr = \"(.*)\"/\1/p" | sed -E "s/0\.0\.0.\0/127.0.0.1/g" )
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

# Check whether the local witnet node is below 20211008 "common checkpoint" (#441649)
if ! $WITNET_BINARY node blockchain --epoch 683499 --limit 1 2>&1 | grep -q "block for epoch"; then
  log "The local witnet node at $HOST seems to be syncing blocks prior to the 20211008 fork. No recovery action is needed"
  exit 0
fi

# Check whether the local witnet node is on the `A` chain, and if so, skip recovery
if $WITNET_BINARY node blockchain --epoch 689769 --limit 2 2>&1 | grep -q '#689769 had digest 90b64edb'; then
  log "The local witnet node at $HOST seems to be on the leading chain. No recovery action is needed"
  exit 0
fi

# There is no way back, recovery is needed
echo "$RECOVERY_BANNER"

# Update known peers in configuration file
log "Updating known_peers in configuration file at $WITNET_CONFIG_FILE"
sed -ziE "s/known_peers\s*=\s*\[\n.*\\,\n\]/known_peers = [$KNOWN_PEERS]/g"  "$WITNET_CONFIG_FILE" &&
log "Successfully updated known_peers in configuration file" ||
log "ERROR: Failed to update known_peers in configuration file at $WITNET_CONFIG_FILE"

# Rewind local chain back to the 20211008 "common checkpoint" (#683499)
log "Triggering rewind of local block chain back to epoch #683499"
$WITNET_BINARY node rewind --epoch 683499 &>/dev/null
REWIND_START=$(date +%s)

# Flush existing peers and inject new peers in runtime
$WITNET_BINARY node clearPeers 2>&1 | grep -q "Successful" &&
log "Successfully cleared existing peers from buckets" ||
log "ERROR: Failed to clear existing peers from buckets"
echo "$KNOWN_PEERS" | sed -r "s/\\\n\s*\"([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+\:[0-9]+)\"\,/\1, /g" | "$WITNET_BINARY" node addPeers 2>&1 | grep -q "Successful" &&
log "Successfully added healthy peers" ||
log "ERROR: Failed to add new list of helthy peers"

# Wait for the rewind to complete, showing progress and ETA
while true
  STATS=$($WITNET_BINARY node nodeStats 2>&1)
  if echo "$STATS" | grep -q "synchronized"; then
    log "Successfully finished rewinding and synchronizing!"
    break
  else
    NOW=$(date +%s)
    PERCENTAGE=$(echo "$STATS" | sed -En "s/.*\:\s*(.*)\.(.*)\%.*/\1.\2%/p")
    PERCENTAGE_RAW=$(echo "$PERCENTAGE" | sed -En "s/0*(.*)\.(.*)\%/\1\2/p")
    log "Still rewinding and synchronizing. Progress: $PERCENTAGE. ETA: $(eta "$REWIND_START" "$NOW" "$PERCENTAGE_RAW")"
    sleep 30
  fi
do true; done
