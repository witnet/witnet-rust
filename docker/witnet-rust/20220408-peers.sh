#!/usr/bin/env bash

WITNET_BINARY=${1:-./witnet}
WITNET_CONFIG_FILE=${2:-./witnet.toml}

PEERS_BANNER="

██╗  ██╗███████╗ █████╗ ██╗  ████████╗██╗  ██╗██╗   ██╗
██║  ██║██╔════╝██╔══██╗██║  ╚══██╔══╝██║  ██║╚██╗ ██╔╝
███████║█████╗  ███████║██║     ██║   ███████║ ╚████╔╝
██╔══██║██╔══╝  ██╔══██║██║     ██║   ██╔══██║  ╚██╔╝
██║  ██║███████╗██║  ██║███████╗██║   ██║  ██║   ██║
╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚══════╝╚═╝   ╚═╝  ╚═╝   ╚═╝
██████╗ ███████╗███████╗██████╗ ███████╗
██╔══██╗██╔════╝██╔════╝██╔══██╗██╔════╝
██████╔╝█████╗  █████╗  ██████╔╝███████╗
██╔═══╝ ██╔══╝  ██╔══╝  ██╔══██╗╚════██║
██║     ███████╗███████╗██║  ██║███████║
╚═╝     ╚══════╝╚══════╝╚═╝  ╚═╝╚══════╝
██╗███╗   ██╗     ██╗███████╗ ██████╗████████╗██╗
██║████╗  ██║     ██║██╔════╝██╔════╝╚══██╔══╝██║
██║██╔██╗ ██║     ██║█████╗  ██║        ██║   ██║
██║██║╚██╗██║██   ██║██╔══╝  ██║        ██║   ╚═╝
██║██║ ╚████║╚█████╔╝███████╗╚██████╗   ██║   ██╗
╚═╝╚═╝  ╚═══╝ ╚════╝ ╚══════╝ ╚═════╝   ╚═╝   ╚═╝

╔══════════════════════════════════════════════════════════╗
║ INJECTING HEALTHY PEERS                                  ║
╠══════════════════════════════════════════════════════════╣
║ This process will inject healthy peer addresses into     ║
║ your node so as to facilitate quick synchronization and  ║
║ ensure that you are connected to the right network.      ║
╟──────────────────────────────────────────────────────────╢
║ This will take only a minute.                            ║
╟──────────────────────────────────────────────────────────╢
║ Feel free to ask any questions on:                       ║
║ Discord:  https://discord.gg/witnet                      ║
║ Telegram: https://t.me/witnetio                          ║
╚══════════════════════════════════════════════════════════╝
"

KNOWN_PEERS='\n    "5.9.5.85:22339",\n    "5.9.20.188:22386",\n    "38.242.204.144:21333",\n    "45.130.104.29:21336",\n    "45.43.29.118:16",\n    "66.94.112.65:21337",\n    "66.94.112.71:21337",\n    "85.208.51.169:21337",\n    "88.198.31.248:22380",\n    "89.58.11.231:21337",\n    "94.130.66.3:22378",\n    "103.219.154.96:21337",\n    "136.243.22.47:22339",\n    "144.76.57.252:22365",\n    "148.251.153.67:22387",\n    "154.53.51.17:21332",\n    "161.97.65.252:21337",\n    "162.55.233.238:22339",\n    "167.86.125.10:21332",\n    "173.249.27.241:21337",\n    "173.249.63.214:21337",\n    "185.208.206.52:21333",\n    "193.26.159.13:21337",\n    "194.163.137.36:21333",\n    "195.201.164.199:22375",\n    "207.180.206.216:21333",\n    "209.126.13.170:21332",\n    "209.145.54.183:21333",\n'

# Just a pretty logging helper
function log {
  echo "[20220408_PEERS] $1"
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

# This script can be skipped by setting environment variable SKIP_20220408_PEERS to "true"
if [[ "$SKIP_WIP20220408_PEERS" == "true" ]]; then
  log "Skipping 20220408 peers injection"
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

# Print the banner
echo "$PEERS_BANNER"

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

# Update known peers in configuration file
log "Updating known_peers in configuration file at $WITNET_CONFIG_FILE"
sed -ziE "s/known_peers\s*=\s*\[\n.*\\,\n\]/known_peers = [$KNOWN_PEERS]/g"  "$WITNET_CONFIG_FILE" &&
log "Successfully updated known_peers in configuration file" ||
log "ERROR: Failed to update known_peers in configuration file at $WITNET_CONFIG_FILE"

# Inject new peers in runtime
echo "$KNOWN_PEERS" | sed -r "s/\\\n\s*\"([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+\:[0-9]+)\"\,/\1, /g" | "$WITNET_BINARY" node addPeers 2>&1 | grep -q "Successful" &&
log "Successfully added healthy peers" ||
log "ERROR: Failed to add new list of healthy peers"
