#!/bin/bash

PUBLIC_ADDR_DISCOVERY=${PUBLIC_ADDR_DISCOVERY:-"true"}

if [[ "$PUBLIC_ADDR_DISCOVERY" == "true" ]]; then
    ./ip_detector.sh
fi

./witnet "$@"