#!/usr/bin/env bash

WITNET_BINARY=${1:-./witnet}
WITNET_CONFIG_FILE=${2:-./witnet.toml}
WITNET_CONFIG_DIR=$(realpath $WITNET_CONFIG_FILE | sed -r 's/(\/[^\/]+){1}$//g')
RUN_FILE=$(realpath "$WITNET_CONFIG_DIR/grinch_run_file")

RECOVERY_BANNER="
                       __..._
                    ,-\`      \`',
                  ,'            \
                 /               |
               ,'       ,        \
             ,'       ,/-'\`       \
         _ ./     ,.'\`/            \
      .-\` \`^\_,.'\`   /              \`\__
      7     /       /   _,._,.,_,.-'.\`  \`\
      \A  __/   ,-\`\`\`-\`\`   \`,   \`,   \`  ,\`)
        ^-\`    /      \`                 ,/
               (        ,   ,_   ,-,_,<\`
                \__ T--\` \`''\` \`\`\`    _,\
                  \_/|\_         ,.-\` | |
                  _/ | |T\_   _,'Y    / +--,_
              <\`\`\`   \_\/_/  \`\\_/   /       \`\
              /  ,--   \` _,--,_\`----\`   _,,_   \
             /  \` |     <_._._ >       \`  \ \`  \\\`
            |     |       ,   \`           |     |
             V|   \       |                |   |\`
              \    \      /               /    /
               \x   \_   |             /-\`    /
                 \    \`-,|        ,/--\`     /\`
                  \x_    \_  /--'\`       , /
                     \x_   \`\`        ,,/\` \`
                        \`-,_,-'   ,'\`
                         _|       |\`\
                        ( \`-\`\`/\`\`/\`_/
                         \`-\`-,.-.-\`

╔══════════════════════════════════════════════════════════╗
║ THE LOCAL CHAIN MAY BE AFFECTED BY THE GRINCH INCIDENT.  ║
║ PROCEEDING TO PERFORM AUTOMATIC RECOVERY.                ║
╠══════════════════════════════════════════════════════════╣
║ This process will sanitize the local chain state by      ║
║ rewinding back to a safe point before the network was    ║
║ severely disrupted, and then continue synchronizing and  ║
║ operating as usual.                                      ║
╟──────────────────────────────────────────────────────────╢
║ This may take from 6 to 12 hours depending on the        ║
║ throughput of your network, CPU, RAM and hard disk.      ║
╟──────────────────────────────────────────────────────────╢
║ Feel free to ask any questions on:                       ║
║ Discord:  https://discord.gg/witnet                      ║
║ Telegram: https://t.me/witnetio                          ║
╚══════════════════════════════════════════════════════════╝
"

KNOWN_PEERS='\n    "3.139.145.178:21337",\n    "18.116.131.252:21337",\n    "71.205.215.52:21337",\n    "95.111.234.78:21337",\n    "173.249.3.178:21337",\n    "173.249.8.65:21337",\n'

# Just a pretty logging helper
function log {
  echo "[GRINCH_RECOVERY] $1"
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

function succeed {
  touch "$RUN_FILE"
  log "Cheer up, dude. It’s Christmas."
  exit 0
}

# This script can be skipped by setting environment variable SKIP_GRINCH_RECOVERY to "true"
if [[ "$SKIP_GRINCH_RECOVERY" == "true" ]]; then
  log "Skipping GRINCH recovery"
  succeed
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

# Skip recovery if it has been performed before
# Recovery can be forced with "FORCE_GRINCH_RECOVERY=true"
if [[ ! "$FORCE_GRINCH_RECOVERY" == "true" ]]; then
  if [ -f "$RUN_FILE" ]; then
    log "The local node is probably in a health state already, as the recovery script was executed successfully before"
    succeed
  fi
fi

# Update peers in configuration file
log "Updating known_peers in configuration file at $WITNET_CONFIG_FILE"
sed -ziE "s/known_peers\s*=\s*\[.*\n\]/known_peers = [$KNOWN_PEERS]/g" "$WITNET_CONFIG_FILE" &&
log "Successfully updated known_peers in configuration file" ||
log "ERROR: Failed to update known_peers in configuration file at $WITNET_CONFIG_FILE"
log "Updating outbound_limit in configuration file at $WITNET_CONFIG_FILE"
sed -ziE "s/outbound_limit\s*=\s*8/outbound_limit = 3/g" "$WITNET_CONFIG_FILE" &&
log "Successfully updated outbound_limit in configuration file" ||
log "ERROR: Failed to update outbound_limit in configuration file at $WITNET_CONFIG_FILE"

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

# Check whether the local witnet node is below GRINCH "safe checkpoint" (#2242439)
if ! $WITNET_BINARY node blockchain --epoch 2242439 --limit 1 2>&1 | grep -q "block for epoch"; then
  log "The local witnet node at $HOST seems to be syncing blocks prior to the GRINCH incident. No recovery action is needed"
  succeed
fi
# Check whether the local witnet node is on the leading chain, and if so, skip recovery
if $WITNET_BINARY node blockchain --epoch 2249490 --limit 2 2>&1 | grep -q '#2249490 had digest 912165ac34'; then
  log "The local witnet node at $HOST seems to be on the leading chain. No recovery action is needed"
  succeed
fi

# There is no way back, recovery is needed
echo "$RECOVERY_BANNER"

# Rewind local chain back to the GRINCH "safe checkpoint" (#2242439)
log "Triggering rewind of local block chain back to epoch #2242439"
$WITNET_BINARY node rewind --epoch 2242439 &>/dev/null
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

succeed
