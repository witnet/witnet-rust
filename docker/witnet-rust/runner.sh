#!/bin/bash

ERROR_BANNER="

 ██████╗██████╗  █████╗ ███████╗██╗  ██╗██╗
██╔════╝██╔══██╗██╔══██╗██╔════╝██║  ██║██║
██║     ██████╔╝███████║███████╗███████║██║
██║     ██╔══██╗██╔══██║╚════██║██╔══██║╚═╝
╚██████╗██║  ██║██║  ██║███████║██║  ██║██╗
 ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚═╝

╔══════════════════════════════════════════════════════════╗
║ WITNET NODE CRASHED AND IS NOW RESTARTING                ║
╠══════════════════════════════════════════════════════════╣
║ Please examine the lines above this banner to get more   ║
║ information about the specific error that brought the    ║
║ node down.                                               ║
╟──────────────────────────────────────────────────────────╢
║ The node will be restarting in 30 seconds.               ║
╟──────────────────────────────────────────────────────────╢
║ If the error happened when starting the node for the     ║
║ first time, this may be caused by some misconfiguration, ║
║ and the error may happen repeatedly until a valid        ║
║ configuration is provided.                               ║
╟──────────────────────────────────────────────────────────╢
║ If the error happened at any other time, please report   ║
║ the error (and any traces printed above) at the          ║
║ witnet-rust issue tracker on GitHub:                     ║
║                                                          ║
║ https://github.com/witnet/witnet-rust/issues             ║
╚══════════════════════════════════════════════════════════╝
"

# Read from environment whether the public address discovery feature is enabled (defaults to true)
PUBLIC_ADDR_DISCOVERY=${PUBLIC_ADDR_DISCOVERY:-"true"}

# Read log level from environment, or default to info if not explicitly specified
LOG_LEVEL=${LOG_LEVEL:-"info"}

# Run the migrator (e.g. move RocksDB data from "./witnet" into "./witnet/storage")
./migrator.sh

# Change directory into the file system root so that all paths are absolute when using "docker exec"
cd /

# Run in a loop so it is automatically restarted upon crashing
while true; do
    # Run the public address detector if enabled
    if [[ "$PUBLIC_ADDR_DISCOVERY" == "true" ]]; then
        /tmp/ip_detector.sh
    fi

    # Run the node itself, using configuration from the default directory and passing down any arguments that may be
    # appended when running "docker run"
    RUST_LOG=witnet=$LOG_LEVEL /tmp/witnet-raw -c /.witnet/config/witnet.toml "$@" || echo "$ERROR_BANNER"
    sleep 30
done
