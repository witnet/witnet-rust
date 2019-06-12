#!/usr/bin/env python3

from testlib import *

execute_tests([
    # Check if the peering and JSONRPC interfaces are up
    port_is_up("127.0.0.1", 21337),
    port_is_up("127.0.0.1", 21338),
    
    # Sleep for 0 seconds. Yes, it does nothing.
    wait(0),
    
    # Open a TCP connection to the local JSONRPC server
    tcp_connect("127.0.0.1", 21338),
    
    # Compose a JSONRPC message for subscribing to new blocks
    jsonrpc_request("witnet_subscribe", ["newBlocks"]),
    # Write the message to the connection
    jsonrpc_write,
    # Read the response
    jsonrpc_read,
    # Check if the a subscription ID was returned
    jsonrpc_check_result(lambda id: int(id) > 0),
    
    # Read a raw JSONRPC message from a file
    read_file("/requests/bitcoin_price.json"),
    # Write the message to the connection
    jsonrpc_write,
    # Read the response
    jsonrpc_read,
    # Check if the response is successful (contains a "result" key)
    jsonrpc_success,
    
    # Wait for a block that contains at least one data request, or fail after 5 blocks
    wait_for_next_block(5, block_contains_any_dr),
    # Check if the block we waited for contains any data request (it obviously does)
    block_contains_any_dr,

    # Close the TCP connection nicely
    tcp_disconnect,
])