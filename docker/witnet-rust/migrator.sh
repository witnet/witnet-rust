#!/bin/bash

WITNET_FOLDER="/.witnet"
WITNET_CONFIG_FOLDER="$WITNET_FOLDER/config"
WITNET_STORAGE_FOLDER="$WITNET_FOLDER/storage"
CONFIG_FILE="./witnet.toml"

function log {
  echo "[MIGRATOR] $1"
}

function migrate_storage {
  OLD_FOLDER="/.witnet"
  find "$OLD_FOLDER" -maxdepth 1 -type f -exec mv -n {} "$WITNET_STORAGE_FOLDER" \;
}

function migrate {
  log "Ensuring that configuration folder '$WITNET_CONFIG_FOLDER' does exist" &&
  mkdir -p "$WITNET_CONFIG_FOLDER" &&
  log "Ensuring that storage folder '$WITNET_STORAGE_FOLDER' does exist" &&
  mkdir -p "$WITNET_STORAGE_FOLDER" &&
  log "Moving configuration files into configuration folder '$WITNET_CONFIG_FOLDER'" &&
  mv "$CONFIG_FILE" "$WITNET_CONFIG_FOLDER/witnet.toml" &&
  cp "genesis_block.json" "$WITNET_CONFIG_FOLDER" &&
  chmod -R 777 "$WITNET_FOLDER/config" &&
  log "Copying old storage (if any) into new storage path" &&
  migrate_storage
}

log "Using configuration from '$CONFIG_FILE'"
migrate