#!/usr/bin/env python3

import json, socket, time
from contextlib import closing
from result import *

context = {
    'last_id': 0
}

def execute_tests(tests, prev=Ok(())):
    if isinstance(tests, list) and len(tests) > 0:
        result = prev.and_then(lambda v: execute_test(tests[0], v))
        return execute_tests(tests[1:], result)
    else:
        return tests

def execute_test(test, prev):
    print("[ %s ]\nRunning test '%s'..." % (test.__name__, test.__name__))
    result = test(prev)
    
    result_string = result.map(lambda v: "✔ Ok: %s" % str(v)).get_or(lambda v: "✘ Err: %s" % v.unwrap_error())

    print("%s\n" % result_string)

    return result

def wait(seconds):
    def wait(prev):
        print(f"\tWaiting {seconds} seconds")
        time.sleep(seconds)
        return Ok(f"the {seconds} seconds wait was totally worth it")

    return wait

def port_is_up(ip, port):
    def port_is_up(prev):
        with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as sock:
            connection_result = sock.connect_ex((ip, port))
            if connection_result == 0:
                return Ok(f"server at {ip}:{port} is up")
            else:
                return Err(f"server at {ip}:{port} is NOT up")

    return port_is_up

def tcp_connect(ip, port):
    def tcp_connect(prev):
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        connection_result = sock.connect_ex((ip, port))
        if connection_result == 0:
            context['sock'] = sock
            return Ok(f"successful TCP connection to {ip}:{port}")
        else:
            return Err(f"failed to open TCP connection to {ip}:{port}")

    return tcp_connect

def tcp_disconnect(prev):
    sock = context.get('sock')
    peer = sock.getpeername()
    sock.close()

    return Ok("connection to %s:%i was closed orderly" % peer)

def read_file(path):
    def read_file(prev):
        with open(path) as file:
            for line in file:
                return Ok(line)

        return Err(f"file at '{request_name}' couldn't be open")
    
    return read_file

def jsonrpc_write(prev):
    sock = context.get('sock')
    sock.sendall(prev.strip().encode() + b'\n')

    return Ok("sent request to %s:%i" % sock.getpeername())

def jsonrpc_check_result(checker_function):
    def jsonrpc_check_result(json):
        if 'result' in json:
            checker_result = checker_function(json.get('result'))
            if checker_result == True:
                return Ok("raw request was successful and the result was positive (result was: %s)" % json.get('result'))
            else:
                return Err("raw request was successfully executed but the result was negative (result was: %s)" % json.get('result'))
        else:
            return Err("raw request failed. Error was: %s" %  json.get('error'))
    
    return jsonrpc_check_result

def jsonrpc_success(prev):
    return jsonrpc_check_result(lambda result: result == True)(prev)

def jsonrpc_request(method, params):
    def jsonrpc_request(prev):
        id = context['last_id']
        context['last_id'] = context['last_id'] + 1
        req = {'jsonrpc': '2.0', 'method': method, 'params': params, 'id': id }

        return Ok(json.dumps(req))

    return jsonrpc_request


def jsonrpc_read(prev):
    sock = context.get('sock')
    raw = sock.recv(20000)
    res = json.loads(raw.decode('utf8'))

    return Ok(res)

def wait_for_next_block(max_retries, filter_function = lambda x: True):
    def wait_for_next_block(prev, retries_left=max_retries):
        def retry(checkpoint, reason, retries_left):
            if retries_left > 0:
                retries_left = retries_left - 1
                print(f"\tIgnoring block for checkpoint {checkpoint} because {reason}. {retries_left} retries left")
                return wait_for_next_block(prev, retries_left)
            else:
                return Err(f"couldn't find a block passing the filter after {max_retries}")

        def decide(res):
            if 'params' in res and isinstance(res['params'], dict) and 'result' in res['params'] and isinstance(res['params']['result'], dict) and 'block_header' in res['params']['result']:
                block = res['params']['result']
                checkpoint = block['block_header']['beacon']['checkpoint']
                return filter_function(block).or_else(lambda reason: retry(checkpoint, reason, retries_left))
            else:
                print("\tIgnored this message while waiting for next block: %s" % str(res))
                return wait_for_next_block(prev)
            
        return jsonrpc_read(prev).and_then(decide)

    return wait_for_next_block

def block_contains_any_dr(prev):
    try:
        checkpoint = prev['block_header']['beacon']['checkpoint']
        drs = prev['txns']['data_request_txns']
        if len(drs) > 0:
            return Ok(prev)
        else:
            return Err("there are no data requests inside")
    except:
        return Err("input does not look like a valid block: %s" % str(prev))

def with_context(test):
    def with_context(prev):
        return test(context, prev)

    with_context.__name__ = test.__name__

    return with_context