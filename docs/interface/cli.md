# Command Line Interface (CLI)

The cli subcommand provides a human friendly command-line interface to the [JSON-RPC API][jsonrpc].

## Usage

See all the available options by running the help command.
`cargo run --` can be used to replace `witnet` in a development environment.

```sh
$ witnet cli --help
$ cargo run -- cli --help
```

The JSON-RPC server address is obtained from the [configuration file][configuration].
The path of this file can be set using the `-c` or `--config` flag.
This flag must appear after `cli`.

```sh
$ witnet cli -c witnet.toml getBlockChain
```

```text
$ witnet cli getBlockChain
Block for epoch #46924 had digest e706995269bfc4fb5f4ab9082765a1bdb48fc6e58cdf5f95621c9e3f849301ed
Block for epoch #46925 had digest 2dc469691916a862154eb92473278ea8591ace910ec7ecb560797cbb91fdc01e
```

If there is any error, the process will return a non-zero exit code.

```text
$ witnet cli getBlockChain
ERROR 2019-01-03T12:01:51Z: witnet: Error: Connection refused (os error 111)
```

The executable implements the usual logging API, which can be enabled using `RUST_LOG=witnet=debug`:

```text
$ RUST_LOG=witnet=debug witnet cli getBlockChain
 INFO 2019-01-03T12:04:43Z: witnet::json_rpc_client: Connecting to JSON-RPC server at 127.0.0.1:21338
ERROR 2019-01-03T12:04:43Z: witnet: Error: Connection refused (os error 111)
```

### Commands

#### raw

The `raw` command allows sending raw JSON-RPC requests from the command line.
It can be used in an interactive way: each line of user input will be sent
to the JSON-RPC server without any modifications:

```sh
$ witnet cli -c witnet.toml raw
```

Each block represents a method call:
the first line is a request, the second line is a response.

```js
hi
{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}
```
```js
{"jsonrpc": "2.0","method": "getBlockChain", "id": 1}
{"jsonrpc":"2.0","result":[[242037,"3f8c9ed0fa721e39de9483f61f290f76a541757a828e54a8d951101b1940c59a"]],"id":1}
```
```js
{"jsonrpc": "2.0","method": "someInvalidMethod", "id": 2}
{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":2}
```
```js
bye
{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}
```


Alternatively, the input can be read from a file using pipes, as is usual in Unix-like environments:

```text
$ cat get_block_chain.txt | witnet cli raw
{"jsonrpc":"2.0","result":[[242037,"3f8c9ed0fa721e39de9483f61f290f76a541757a828e54a8d951101b1940c59a"]],"id":1}
```

#### getBlockChain

Returns the hashes of all the blocks in the blockchain, one per line:

```text
$ witnet cli getBlockChain -c witnet_01.toml
Block for epoch #46924 had digest e706995269bfc4fb5f4ab9082765a1bdb48fc6e58cdf5f95621c9e3f849301ed
Block for epoch #46925 had digest 2dc469691916a862154eb92473278ea8591ace910ec7ecb560797cbb91fdc01e
```

#### getDataRequest

Returns the data request that matches with the provided output pointer.

```text
$ witnet cli getDataRequest --outputPointer 1234567890abcdef111111111111111111111111111111111111111111111111:1
```
```js
{"jsonrpc":"2.0","result":{"DataRequest":{"backup_witnesses":0,"commit_fee":0,"data_request":{"aggregate":{"script":[0]},"consensus":{"script":[0]},"deliver":[{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l2awcd/"}],"not_before":0,"retrieve":[{"kind":"HTTP-GET","script":[0],"url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22"}]},"pkh":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"reveal_fee":0,"tally_fee":0,"time_lock":0,"value":0,"witnesses":0}},"id":"1"}
```

The way to provide a valid outputPointer the format is: a 32 hex digits for transaction_id, a semicolon and the output index: `{transaction_id}:{output_index}` 

[jsonrpc]: json-rpc/
[configuration]: ../configuration/toml-file/