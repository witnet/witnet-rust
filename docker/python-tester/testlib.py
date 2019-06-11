#!/usr/bin/env python3

import json, os, socket, sys, time
from contextlib import closing
from result import *

context = {
    'last_id': 0
}

def print_success_banner(_):
    print(r''' _____                             _ 
/  ___|                           | |
\ `--. _   _  ___ ___ ___  ___ ___| |
 `--. \ | | |/ __/ __/ _ \/ __/ __| |
/\__/ / |_| | (_| (_|  __/\__ \__ \_|
\____/ \__,_|\___\___\___||___/___(_)
''')

def abort(_):
    sys.exit(1)

def execute_tests(tests, prev=Ok(())):
    if isinstance(tests, list) and len(tests) > 0:
        result = prev.and_then(lambda v: execute_test(tests[0], v))
        prev.or_else(abort)
        return execute_tests(tests[1:], result)
    else:
        prev.and_then(print_success_banner)
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

def process_is_running(process_name):
    def process_is_running(prev):
        ps = os.popen("ps -Af").read()
        count = ps.count(process_name)

        if count > 0:
            return Ok(f"process '{process_name}' is running")
        else:
            return Err(f"process '{process_name}' is not running")

    return process_is_running

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

def json_parse(prev):
    try:
        return Ok(json.loads(prev))
    except ValueError as e:
        return Err(f"couldn't parse input string as JSON. Trace: {e}")
    else:
        return Err("couldn't parse input string as JSON")

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

def wait_for_next_block(max_retries = 0, filter_function = lambda block, prev: Ok(block)):
    def wait_for_next_block(prev, retries_left=max_retries):
        def retry(checkpoint, reason, retries_left):
            if retries_left > 1:
                retries_left = retries_left - 1
                print(f"\tIgnoring block for checkpoint {checkpoint} because {reason}. {retries_left} retries left")
                return wait_for_next_block(prev, retries_left)
            else:
                return Err(f"couldn't find a block passing the filter after {max_retries} retries")

        def decide(res):
            if 'params' in res and isinstance(res['params'], dict) and 'result' in res['params'] and isinstance(res['params']['result'], dict) and 'block_header' in res['params']['result']:
                block = res['params']['result']
                checkpoint = block['block_header']['beacon']['checkpoint']
                return filter_function(block, prev).or_else(lambda reason: retry(checkpoint, reason, retries_left))
            else:
                print("\tIgnored this message while waiting for next block: %s" % str(res))
                return wait_for_next_block(prev)
            
        return jsonrpc_read(prev).and_then(decide)

    wait_for_next_block.__name__ = f'{wait_for_next_block.__name__}({filter_function.__name__})'

    return wait_for_next_block

def block_contains_dr(block, request):
    try:
        checkpoint = block['block_header']['beacon']['checkpoint']
        drs = block['txns']['data_request_txns']
        if len(drs) > 0:
            matches = [dr for dr in drs if dr['body']['dr_output'] == request['params']['dro']]
            if len(matches) > 0:
                return Ok(block)
            else:
                return Err(f"the data request was not included")
        else:
            return Err("there are no data requests inside")
    except:
        return Err(f"unknown error.\n\tBlock was {str(block)}\n\tRequest was {str(request)}")

def block_contains_commitments_for_dr(block, request):
    try:
        checkpoint = block['block_header']['beacon']['checkpoint']
        commits = block['txns']['commit_txns']
        if len(commits) > 0:
            matches = [tx for tx in commits if True]
            if len(matches) > 0:
                rf = request['params']['dro']['witnesses']
                if len(matches) >= rf:
                    return Ok(block)
                else:
                    return Err(f"there are not enough commitments ({len(matches)} < {rf})")
            else:
                return Err(f"there are no commitments for the specific data request")
        else:
            return Err("there are no commitments inside")
    except ValueError as e:
        return Err(f"ValueError: {e}")
    else:
        return Err(f"unknown error.\n\tBlock was {str(block)}\n\tRequest was {str(request)}")

def block_contains_reveals_for_dr(block, request):
    try:
        checkpoint = block['block_header']['beacon']['checkpoint']
        reveals = block['txns']['reveal_txns']
        if len(reveals) > 0:
            matches = [tx for tx in reveals if True]
            if len(matches) > 0:
                rf = request['params']['dro']['witnesses']
                if len(matches) >= rf:
                    return Ok(block)
                else:
                    return Err(f"there are not enough reveals ({len(matches)} < {rf})")
            else:
                return Err(f"there are no reveals for the specific data request")
        else:
            return Err("there are no reveals inside")
    except ValueError as e:
        return Err(f"ValueError: {e}")
    except:
        return Err(f"unknown error.\n\tBlock was {str(block)}\n\tRequest was {str(request)}")

def block_contains_tally_for_dr(block, request):
    try:
        checkpoint = block['block_header']['beacon']['checkpoint']
        tallies = block['txns']['tally_txns']
        if len(tallies) > 0:
            matches = [tx for tx in tallies if True]
            if len(matches) == 1:
                return Ok(block)
            elif len(matches) > 1:
                return Err(f"there are too many tallies for the specific data request ({len(matches)})")
            else:
                return Err("there is no tally for the specific data request")
        else:
            return Err("there are no tallies inside")
    except ValueError as e:
        return Err(f"ValueError: {e}")
    except:
        return Err(f"unknown error.\n\tBlock was {str(block)}\n\tRequest was {str(request)}")

def poll(inner_function, period=1, max_retries=5):
    def poll(prev, retries_left=max_retries):
        def retry(reason, retries_left=max_retries):
            if retries_left > 1:
                retries_left = retries_left - 1
                print(f"\tWill retry test '{inner_function.__name__}' in {period}s because {reason}. {retries_left} retries left")
                time.sleep(period)
                return poll(prev, retries_left)
            else:
                return Err(f"test '{inner_function.__name__}' didn't succeed after {max_retries} retries")

        return inner_function(prev).or_else(lambda reason: retry(reason, retries_left))

    poll.__name__ = inner_function.__name__

    return poll

def into_context(key):
    def into_context(val):
        context[key] = val
        return Ok(val)

    return into_context

def from_context(key):
    def from_context(prev):
        val = context.get(key)
        if val:
            return Ok(val)
        else:
            return Err(f"no key '{key}' in context")
    
    return from_context

def with_context(test):
    def with_context(prev):
        return test(context, prev)

    with_context.__name__ = test.__name__

    return with_context