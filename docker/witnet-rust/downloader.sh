#!/bin/bash

VERSION=${WITNET_VERSION:-"latest"}

function log {
  echo "[DOWNLOADER] $1"
}

if [[ "$VERSION" == "latest" ]]; then
    VERSION=$(curl https://github.com/witnet/witnet-rust/releases/latest --cacert /etc/ssl/certs/ca-certificates.crt 2>/dev/null | egrep -o "[0-9|\.]{5}(-rc[0-9]+)?")
fi

TRIPLET=$(bash --version | head -1 | sed -En 's/^.*\ \((.+)-(.+)-(.+)\)$/\1-\2-\3/p')

if [[ "$TRIPLET" == *"linux"* ]]; then
    TRIPLET=${TRIPLET/pc/unknown}
fi

URL="https://github.com/witnet/witnet-rust/releases/download/$VERSION/witnet-$VERSION-$TRIPLET.tar.gz"

FILENAME="$VERSION.tar.gz"
WITNET_FOLDER="/.witnet"

log "Downloading 'witnet-$VERSION-$TRIPLET.tar.gz'. It may take a few seconds..."
curl -L "$URL" -o "/tmp/$FILENAME" --cacert /etc/ssl/certs/ca-certificates.crt >/dev/null 2>&1 &&
tar -zxf "/tmp/$FILENAME" --directory . >/dev/null 2>&1 &&
chmod +x ./witnet &&
cp ./witnet /usr/local/bin/ &&
rm -f "/tmp/$FILENAME" &&
witnet --version ||
log "Error downloading and installing witnet-rust on version $VERSION for $TRIPLET"
