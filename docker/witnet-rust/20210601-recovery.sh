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
║ https://github.com/witnet/WIPs/blob/master/20210601.md   ║
╟──────────────────────────────────────────────────────────╢
║ Feel free to ask any questions on:                       ║
║ Discord:  https://discord.gg/X4uurfP                     ║
║ Telegram: https://t.me/witnetio                          ║
╚══════════════════════════════════════════════════════════╝
"

KNOWN_PEERS='\n    "23.95.164.163:21337",\n    "31.25.98.38:21337",\n    "45.43.30.195:5",\n    "45.43.30.198:17",\n    "45.43.30.200:28",\n    "45.43.30.203:41",\n    "45.154.212.2:3",\n    "45.154.212.7:36",\n    "45.154.212.9:45",\n    "45.154.212.9:50",\n    "45.154.212.11:68",\n    "45.154.212.51:1",\n    "45.154.212.54:20",\n    "45.154.212.56:26",\n    "45.154.212.58:40",\n    "45.154.212.61:51",\n    "45.154.212.62:59",\n    "46.4.102.43:22350",\n    "46.4.115.118:22368",\n    "49.12.133.160:22380",\n    "52.166.178.145:21337",\n    "65.21.148.88:23337",\n    "65.21.150.191:22337",\n    "65.21.154.49:22337",\n    "65.21.157.140:21337",\n    "65.21.157.211:23337",\n    "65.21.158.201:21337",\n    "65.21.185.3:21332",\n    "65.21.185.3:21337",\n    "65.21.185.175:21334",\n    "65.21.185.175:21336",\n    "65.21.185.175:22337",\n    "65.21.185.235:21330",\n    "65.21.185.235:21331",\n    "65.21.185.235:21334",\n    "65.21.185.237:22337",\n    "65.21.187.246:21337",\n    "65.21.187.247:21335",\n    "65.21.187.248:21332",\n    "65.21.187.247:22337",\n    "65.21.187.248:21334",\n    "65.21.187.249:21337",\n    "65.21.187.249:22337",\n    "82.213.200.249:21337",\n    "88.99.68.109:22382",\n    "78.46.83.214:22371",\n    "78.46.86.104:22375",\n    "78.46.123.25:22339",\n    "88.99.208.52:22339",\n    "88.198.8.177:22380",\n    "93.100.156.159:21337",\n    "95.216.214.204:21337",\n    "95.216.214.238:21337",\n    "95.217.144.154:22355",\n    "104.218.233.115:2",\n    "104.218.233.115:5",\n    "104.218.233.116:6",\n    "104.218.233.117:12",\n    "104.218.233.118:18",\n    "104.218.233.118:20",\n    "104.218.233.119:24",\n    "104.218.233.119:2237",\n    "104.218.233.120:28",\n    "104.218.233.120:30",\n    "104.218.233.121:34",\n    "104.218.233.121:35",\n    "104.218.233.122:36",\n    "116.202.131.26:22339",\n    "116.202.162.85:22349",\n    "116.202.218.95:22344",\n    "116.202.218.95:22359",\n    "135.181.6.61:22344",\n    "135.181.6.61:22352",\n    "135.181.19.225:22340",\n    "135.181.19.227:22381",\n    "135.181.60.153:22350",\n    "135.181.152.251:23337",\n    "136.243.74.201:22346",\n    "136.243.93.163:22349",\n    "136.243.93.244:22373",\n    "136.243.94.119:22379",\n    "136.243.144.199:21337",\n    "138.201.65.62:22355",\n    "138.201.66.37:22342",\n    "138.201.83.20:22362",\n    "138.201.83.56:22350",\n    "144.76.222.234:21337",\n    "148.251.152.217:22370",\n    "159.69.68.67:22351",\n    "159.69.74.89:22339",\n    "159.69.146.71:21337",\n    "168.119.5.23:22362",\n    "168.119.5.24:22344",\n    "168.119.5.25:22357",\n    "173.249.3.178:21337",\n    "173.249.3.178:22337",\n    "173.249.8.65:20337",\n    "173.249.8.65:21337",\n    "178.63.67.40:22350",\n    "188.40.90.184:22339",\n    "188.40.123.177:22354",\n    "195.201.157.91:22339",\n    "213.239.194.162:22369",\n'

# Just a pretty logging helper
function log {
  echo "[20210601_RECOVERY] $1"
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

# This script can be skipped by setting environment variable SKIP_20210601_RECOVERY to "true"
if [[ "$SKIP_WIP20210601_RECOVERY" == "true" ]]; then
  log "Skipping 20210601 recovery"
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

# Check whether the local witnet node is below 20210601 "common checkpoint" (#441649)
if ! $WITNET_BINARY node blockchain --epoch 441649 --limit 1 2>&1 | grep -q "block for epoch"; then
  log "The local witnet node at $HOST seems to be syncing blocks prior to the 20210601 fork. No recovery action is needed"
  exit 0
fi

# Check whether the local witnet node is on the `A` chain, and if so, skip recovery
if $WITNET_BINARY node blockchain --epoch 441649 --limit 2 2>&1 | grep -q '#441649 had digest 46e5dadb'; then
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

# Rewind local chain back to the 20210601 "common checkpoint" (#441649)
log "Triggering rewind of local block chain back to epoch #441649"
$WITNET_BINARY node rewind --epoch 441649 &>/dev/null
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