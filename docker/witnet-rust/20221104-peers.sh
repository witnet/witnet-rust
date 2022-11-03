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

KNOWN_PEERS='\n    "20.120.248.2:21337",\n    "20.126.70.77:21337",\n    "20.126.70.77:21339",\n    "20.126.70.77:21340",\n    "37.187.157.144:21337",\n    "45.13.105.123:21337",\n    "45.130.104.42:24337",\n    "45.130.104.44:23337",\n    "45.130.104.44:25337",\n    "45.130.104.44:27337",\n    "45.130.104.47:27337",\n    "45.130.104.48:26337",\n    "51.77.222.83:21338",\n    "51.91.11.234:21337",\n    "51.91.11.234:21339",\n    "51.195.253.112:21338",\n    "51.195.253.112:21339",\n    "64.227.97.213:21342",\n    "89.58.9.110:21337",\n    "94.130.51.165:21337",\n    "94.130.51.168:21337",\n    "94.130.51.169:21337",\n    "94.130.51.248:21337",\n    "94.130.69.228:21337",\n    "149.102.130.64:21337",\n    "157.90.131.55:21337",\n    "161.97.168.165:21337",\n    "173.249.3.178:21337",\n    "173.249.3.178:22337",\n    "173.249.8.65:20337",\n    "173.249.8.65:21337",\n    "176.9.139.59:21370",\n    "176.9.44.2:21337",\n    "176.9.44.5:21337",\n    "127.0.0.1:21337",\n    "135.125.106.137:21338",\n    "137.184.75.162:21337",\n    "142.132.254.240:21337",\n    "142.132.254.241:21337",\n    "142.132.254.242:21337",\n    "149.102.130.64:21337",\n    "161.97.168.165:21337",\n    "161.97.168.165:22337",\n    "161.97.168.165:23337",\n    "172.104.124.75:21337",\n    "176.9.44.5:21337",\n    "176.9.45.15:21337",\n    "176.9.139.59:21337",\n    "176.9.183.46:21337",\n    "176.98.104.114:21337",\n    "185.213.175.251:21337",\n    "185.213.175.251:21339",\n    "193.26.157.210:21337",\n    "194.39.206.124:21337",\n    "194.163.152.212:21337",\n    "202.61.237.253:21337",\n    "207.148.21.22:21337",\n    "207.180.244.229:22337",\n    "207.180.244.229:25337",\n    "207.180.244.229:26337",\n    "207.244.237.163:21337",\n'

# Just a pretty logging helper
function log {
  echo "[20221104_PEERS] $1"
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

# This script can be skipped by setting environment variable SKIP_20221104_PEERS to "true"
if [[ "$SKIP_WIP20221104_PEERS" == "true" ]]; then
  log "Skipping 20221104 peers injection"
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
