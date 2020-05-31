#!/bin/bash

CONFIG_FILE_FROM_CMD=$(echo "$@" | sed -E 's/(.*-c\s*)?(.*\.toml)?.*/\2/')
WITNET_FOLDER="/.witnet"
WITNET_CONFIG_FOLDER="$WITNET_FOLDER/config"
WITNET_STORAGE_FOLDER="$WITNET_FOLDER/storage"
CONFIG_FILE=${CONFIG_FILE_FROM_CMD:-./witnet.toml}

function log {
  echo "[MIGRATOR] $1"
}

function migrate_storage {
  OLD_FOLDER="/.witnet"
  find "$OLD_FOLDER" -type f -maxdepth 1 -exec cp -n {} "$WITNET_STORAGE_FOLDER" \;
}

function migrate {
  log "Ensuring that configuration folder '$WITNET_CONFIG_FOLDER' does exist" &&
  mkdir -p "$WITNET_CONFIG_FOLDER" &&
  log "Ensuring that storage folder '$WITNET_STORAGE_FOLDER' does exist" &&
  mkdir -p "$WITNET_STORAGE_FOLDER" &&
  log "Moving configuration files into configuration folder '$WITNET_FOLDER/config'" &&
  cp -n "$CONFIG_FILE" "$WITNET_CONFIG_FOLDER" &&
  cp "genesis_block.json" "$WITNET_CONFIG_FOLDER" &&
  chmod -R 777 "$WITNET_FOLDER/config" &&
  log "Copying old storage (if any) into new storage path" &&
  migrate_storage
}

log "Using configuration from '$CONFIG_FILE'"
migrate