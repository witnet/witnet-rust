#!/bin/bash

CONFIG_FILE_FROM_CMD=`echo "$@" | sed -E 's/(.*-c\s*)?(.*\.toml)?.*/\2/'`
CONFIG_FILE=${CONFIG_FILE_FROM_CMD:-witnet.toml}
DEFAULT_IP="0.0.0.0"
DEFAULT_PORT="21337"
DEFAULT_ADDR="$DEFAULT_IP:$DEFAULT_PORT"

echo "Reading configuration from $CONFIG_FILE"

function read_public_addr_from_config {
    echo "Reading public_addr from config file";
    PUBLIC_ADDR_FROM_CONFIG=`grep public_addr $CONFIG_FILE | cut -d "\"" -f2`;
    LISTENING_PORT_FROM_CONFIG=`grep "server_addr" $CONFIG_FILE | head -1 | cut -d "\"" -f2 | cut -d ":" -f2`
}

function guess_public_addr {
    echo "Trying to guess public_addr";
    API_URL="http://bot.whatismyipaddress.com/";
    PUBLIC_ADDR_FROM_API="`curl $API_URL 2>/dev/null || echo $DEFAULT_IP`:${LISTENING_PORT_FROM_CONFIG:-$DEFAULT_PORT}";
}

function replace_ip_in_config_if_not_set {
    read_public_addr_from_config;
    if [[ "$PUBLIC_ADDR_FROM_CONFIG" == "$DEFAULT_ADDR" ]]; then
        guess_public_addr;
        if [[ "$PUBLIC_ADDR_FROM_API" != "$DEFAULT_ADDR" ]]; then
           echo "Trying to replace public_address ($PUBLIC_ADDR_FROM_API) into config file ($CONFIG_FILE)";
           sed -i -E "s/public_addr\s*=\s*\"$DEFAULT_ADDR\"/public_addr = \"$PUBLIC_ADDR_FROM_API\"/" $CONFIG_FILE;
        fi
    else
      if [[ "$PUBLIC_ADDR_FROM_CONFIG" == "" ]]; then
        guess_public_addr;
        echo "Trying to write public_address ($PUBLIC_ADDR_FROM_API) into config file ($CONFIG_FILE)";
        sed -i -E "s/^\[connections\]$/[connections]\npublic_addr = \"$PUBLIC_ADDR_FROM_API\"/" $CONFIG_FILE;
      fi
    fi
    return 0; # This is best effort, it's a pity if it didn't work out, but we need to keep running the node anyway.
}

replace_ip_in_config_if_not_set