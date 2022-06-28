#!/bin/bash

VERSION=${WITNET_VERSION:-"latest"}

function log {
  echo "[DOWNLOADER] $1"
}

if [[ "$VERSION" == "latest" ]]; then
    VERSION=$(curl https://api.github.com/repos/witnet/witnet-rust/releases/latest -s | jq .tag_name | cut -d '"' -f 2)
fi

TRIPLET=$(bash --version | head -1 | sed -En 's/^.*\ \((.+)-(.+)-(.+)\)$/\1-\2-\3/p')

if [[ "$TRIPLET" == *"linux"* ]]; then
    TRIPLET=${TRIPLET/pc/unknown}
fi

URL="https://github.com/witnet/witnet-rust/releases/download/$VERSION/witnet-$VERSION-$TRIPLET.tar.gz"

FILENAME="$VERSION.tar.gz"

# Download and extract release bundle
log "Downloading 'witnet-$VERSION-$TRIPLET.tar.gz'. It may take a few seconds..."
curl -L "$URL" -o "/tmp/$FILENAME" --cacert /etc/ssl/certs/ca-certificates.crt &&
tar -zxf "/tmp/$FILENAME" --directory "/tmp/" &&
# Rename the actual binary to 'witnet-raw'
mv witnet witnet-raw &&
chmod +x ./witnet-raw &&
# Make executer.sh hijack command './witnet'
cp ./executer.sh ./witnet &&
# Make executer.sh hijack command 'witnet'
cp ./executer.sh /usr/local/bin/witnet &&
# Delete release bundle
rm -f "/tmp/$FILENAME" &&
witnet --version ||
(log "Error downloading and installing witnet-rust on version $VERSION for $TRIPLET" && exit 1)
