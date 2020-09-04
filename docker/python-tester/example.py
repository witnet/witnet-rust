#!/usr/bin/env python3

import re, time
from testlib import *

def is_hash(input):
    return len(re.findall(r"([a-fA-F\d]{64})", input)) == 1

execute_tests([
    # Wait for the JSONRPC interface to be up (will retry every 2 seconds up to 10 times)
    poll(port_is_up("127.0.0.1", 21338), 2, 10),
    # Check if peering interface is also up
    port_is_up("127.0.0.1", 21337),

    # Sleep for 0 seconds. Yes, it does nothing, it's here only as an example.
    wait(0),

    # Open a TCP connection to the local JSONRPC server
    tcp_connect("127.0.0.1", 21338),

    # Compose a JSONRPC message for subscribing to new blocks
    jsonrpc_request("witnet_subscribe", ["blocks"]),
    # Write the message to the connection
    jsonrpc_write,
    # Read the response
    jsonrpc_read,
    # Check if the a subscription ID was returned
    jsonrpc_check_result(lambda id: int(id) > 0),

    # Wait for the first block notification, which signals that we are in sync
    wait_for_next_block(),

    # Read a raw JSONRPC message from a file
    read_file("/requests/bitcoin_price.json"),
    # Put it into context
    into_context("bitcoin_price_json"),
    # Write the message to the connection
    jsonrpc_write,
    # Read the response
    jsonrpc_read,
    # Check if the response is successful
    jsonrpc_check_result(is_hash),

    # Bring the request JSON from the context
    from_context("bitcoin_price_json"),
    # Parse the JSON
    json_parse,
    # Put it into context
    into_context("bitcoin_price"),
    # Wait for a block that contains at least one data request, or fail after 3 blocks
    wait_for_next_block(3, block_contains_dr),
    # Bring the request from the context
    from_context("bitcoin_price"),
    # Wait for a block that contains enough commitments for the data request, or fail after 3 blocks
    wait_for_next_block(3, block_contains_commitments_for_dr),
    # Bring the request from the context
    from_context("bitcoin_price"),
    # Wait for a block that contains enough reveals for the data request, or fail after 3 blocks
    wait_for_next_block(3, block_contains_reveals_for_dr),
    # Bring the request from the context
    from_context("bitcoin_price"),
    # Wait for a block that contains enough tally for the data request, or fail after 3 blocks
    wait_for_next_block(3, block_contains_tally_for_dr),

    # Close the TCP connection nicely
    tcp_disconnect,
])
