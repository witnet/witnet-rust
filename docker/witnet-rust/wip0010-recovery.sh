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
║ https://github.com/witnet/WIPs/blob/master/wip-0010.md   ║
╟──────────────────────────────────────────────────────────╢
║ Feel free to ask any questions on:                       ║
║ Discord:  https://discord.gg/X4uurfP                     ║
║ Telegram: https://t.me/witnetio                          ║
╚══════════════════════════════════════════════════════════╝
"

KNOWN_PEERS='\n    "5.189.172.149:24337",\n    "46.4.104.114:22339",\n    "49.12.2.247:21337",\n    "64.227.69.233:22339",\n    "64.227.74.14:22339",\n    "68.183.202.131:22339",\n    "78.46.86.104:22342",\n    "78.47.209.194:22339",\n    "81.30.157.7:22339",\n    "81.30.157.7:22345",\n    "81.30.157.7:22346",\n    "81.30.157.7:22348",\n    "81.30.157.7:22350",\n    "81.30.157.7:22351",\n    "81.30.157.7:22352",\n    "81.30.157.7:22353",\n    "85.114.132.66:22339",\n    "85.114.132.66:22348",\n    "85.114.132.66:22351",\n    "85.114.132.66:22352",\n    "94.130.165.149:22339",\n    "94.130.30.59:22339",\n    "95.217.181.71:21337",\n    "104.248.166.16:22339",\n    "116.202.116.196:22339",\n    "116.202.131.166:22339",\n    "116.202.131.24:22339",\n    "116.202.131.25:22339",\n    "116.202.131.30:22339",\n    "116.202.131.31:22339",\n    "116.202.131.32:22339",\n    "116.202.164.176:22339",\n    "116.202.35.247:22339",\n    "134.122.116.156:22339",\n    "134.209.84.237:22339",\n    "135.181.0.31:22339",\n    "135.181.194.161:21337",\n    "135.181.60.149:22345",\n    "136.243.135.114:22339",\n    "136.243.93.142:22352",\n    "136.243.94.171:22346",\n    "136.243.95.38:22342",\n    "136.243.95.38:22348",\n    "138.68.108.254:22339",\n    "142.93.42.49:22339",\n    "144.76.18.98:21337",\n    "144.91.113.168:21337",\n    "148.251.128.18:22348",\n    "148.251.128.19:22342",\n    "148.251.128.26:22341",\n    "157.245.171.146:21337",\n    "159.69.139.239:22339",\n    "159.69.56.28:22339",\n    "159.69.72.123:22339",\n    "159.69.74.96:22339",\n    "159.89.5.213:22339",\n    "159.89.7.193:22339",\n    "161.35.167.68:22339",\n    "161.35.235.109:22339",\n    "163.172.131.197:21337",\n    "165.232.106.11:22339",\n    "165.232.108.246:22339",\n    "165.232.32.106:22339",\n    "167.172.29.131:22339",\n    "167.172.39.205:22339",\n    "167.71.2.204:22339",\n    "167.71.5.253:22339",\n    "167.99.154.252:22339",\n    "167.99.200.189:22339",\n    "167.99.243.125:22339",\n    "167.99.246.148:22339",\n    "167.99.248.158:22339",\n    "167.99.249.236:22339",\n    "173.212.241.42:21337",\n    "173.249.40.145:25337",\n    "176.9.29.25:22342",\n    "178.62.221.245:22339",\n    "188.166.109.122:22339",\n    "188.166.59.159:22339",\n    "188.166.71.96:22339",\n    "188.40.103.83:21537",\n    "188.40.103.83:21637",\n    "188.40.103.83:22437",\n    "188.40.103.83:22737",\n    "188.40.131.24:22339",\n    "188.40.94.105:22339",\n    "192.241.148.38:22339",\n    "195.201.167.113:22339",\n    "195.201.173.77:22339",\n    "195.201.181.221:22339",\n    "195.201.181.245:22339",\n    "195.201.240.189:22339",\n    "206.189.63.220:22339",\n    "207.154.214.111:22339",\n    "207.154.238.90:22339",\n    "207.154.253.202:22339",\n    "207.154.254.153:22339",\n    "213.239.234.132:22348",\n'

# Just a pretty logging helper
function log {
  echo "[WIP0010_RECOVERY] $1"
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
    REMAINING_TIME=$((REMAINING_PROGRESS * 1000 / SPEED ))
    echo $(( REMAINING_TIME / 60 )) minutes $((REMAINING_TIME % 60)) seconds
  fi
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

# Check whether the local witnet node is below WIP-0010 "common checkpoint" (#248839)
if ! $WITNET_BINARY node blockchain --epoch 248839 --limit 1 2>&1 | grep -q "block for epoch"; then
  log "The local witnet node at $HOST seems to be syncing blocks prior to the WIP-0010 fork. No recovery action is needed"
  exit 0
fi

# Check whether the local witnet node is on the `A` chain, and if so, skip recovery
if $WITNET_BINARY node blockchain --epoch 248839 --limit 2 2>&1 | grep -q '#248921 had digest 7556670d'; then
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

# Rewind local chain back to the WIP-0010 "common checkpoint" (#248839)
log "Triggering rewind of local block chain back to epoch #248839"
$WITNET_BINARY node rewind --epoch 248839 &>/dev/null
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