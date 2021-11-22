#!/bin/bash

WITNET_FOLDER="/.witnet"
CONFIG_FILE_FROM_CMD=$(echo "$@" | sed -E 's/(.*-c\s*)?(.*\.toml)?.*/\2/')
CONFIG_FILE=${CONFIG_FILE_FROM_CMD:-$WITNET_FOLDER/config/witnet.toml}
DEFAULT_IP="0.0.0.0"
DEFAULT_PORT="21337"
DEFAULT_ADDR="$DEFAULT_IP:$DEFAULT_PORT"

function log {
  echo "[IP_DETECTOR] $1"
}

function read_public_addr_from_config {
    log "Reading 'public_addr' from config file";
    PUBLIC_ADDR_FROM_CONFIG=$(grep public_addr "$CONFIG_FILE" | cut -d "\"" -f2);
    LISTENING_PORT_FROM_CONFIG=$(grep "server_addr" "$CONFIG_FILE" | head -1 | cut -d "\"" -f2 | cut -d ":" -f2)
}

function guess_public_addr {
    log "Trying to guess 'public_addr'";
    API_URL="https://api.ipify.org";
    PUBLIC_ADDR_FROM_API="$(curl --ipv4 $API_URL 2>/dev/null || echo $DEFAULT_IP):${LISTENING_PORT_FROM_CONFIG:-$DEFAULT_PORT}";
    log $PUBLIC_ADDR_FROM_API
}

function replace_ip_in_config_if_not_set {
    read_public_addr_from_config;
    if [[ "$PUBLIC_ADDR_FROM_CONFIG" == "$DEFAULT_ADDR" ]]; then
        guess_public_addr;
        if [[ "$PUBLIC_ADDR_FROM_API" != "$DEFAULT_ADDR" ]]; then
           log "Trying to replace 'public_address' ($PUBLIC_ADDR_FROM_API) into config file ($CONFIG_FILE)";
           sed -i -E "s/public_addr\s*=\s*\"$DEFAULT_ADDR\"/public_addr = \"$PUBLIC_ADDR_FROM_API\"/" "$CONFIG_FILE";
        fi
    else
      if [[ "$PUBLIC_ADDR_FROM_CONFIG" == "" ]]; then
        guess_public_addr;
        log "Trying to write 'public_address' ($PUBLIC_ADDR_FROM_API) into config file ($CONFIG_FILE)";
        sed -i -E "s/^\[connections\]$/[connections]\npublic_addr = \"$PUBLIC_ADDR_FROM_API\"/" "$CONFIG_FILE";
      fi
    fi
    return 0; # This is best effort, it's a pity if it didn't work out, but we need to keep running the node anyway.
}

log "Using configuration from '$CONFIG_FILE'"
replace_ip_in_config_if_not_set