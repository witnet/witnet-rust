#!/bin/bash

VERSION=$1
COMPONENT=$2
shift
shift

if [ "$VERSION" == "latest" ]; then
    VERSION=`curl https://github.com/witnet/witnet-rust/releases/latest --cacert /etc/ssl/certs/ca-certificates.crt 2>/dev/null | egrep -o "[0-9|\.]{5}"`
fi

TRIPLET=`bash --version | head -1 | sed -En 's/^.*\ \((.+)-(.+)-(.+)\)$/\1-\2-\3/p'`

if [[ "$TRIPLET" == *"linux"* ]]; then
    TRIPLET=`echo $TRIPLET | sed 's/pc/unknown/g'`
fi

URL="https://github.com/witnet/witnet-rust/releases/download/$VERSION/witnet-$VERSION-$TRIPLET.tar.gz"

FILENAME="$VERSION.tar.gz"
FOLDERNAME="."

echo "Downloading 'witnet-$VERSION-$TRIPLET.tar.gz'. It may take a few seconds..."
curl -L $URL -o /tmp/$FILENAME --cacert /etc/ssl/certs/ca-certificates.crt >/dev/null 2>&1 &&
tar -zxf /tmp/$FILENAME --directory $FOLDERNAME >/dev/null 2>&1 &&
chmod +x $FOLDERNAME/witnet &&
$FOLDERNAME/witnet $COMPONENT $@ ||
echo "Error downloading and running a witnet-rust $COMPONENT on version $VERSION for $TRIPLET"
