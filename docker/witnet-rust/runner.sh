#!/bin/bash

PUBLIC_ADDR_DISCOVERY=${PUBLIC_ADDR_DISCOVERY:-"true"}

./migrator.sh

if [[ "$PUBLIC_ADDR_DISCOVERY" == "true" ]]; then
    ./ip_detector.sh
fi

cd /

/tmp/witnet-raw -c /.witnet/config/witnet.toml  "$@"