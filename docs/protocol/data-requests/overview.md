# Data requests

Data requests are the cornerstone of the Witnet protocol. They allow
**clients** to have **witness** nodes **retrieve**, **aggregate** and
**deliver** data on their behalf on demand.

## Request life cycle

Once a data request has been published by a client, it will go through 4
distinct stages: ***retrieval***, ***aggregation***, ***tally*** and
***delivery***. These stages are linear and constitute a single,
unidirectional data flow.

```
╔═════════╗    ╔════════════════════════════╗    ╔═══════════╗    ╔═════════╗
║ Client  ║    ║ Witnesses                  ║    ║ Miner     ║    ║ Bridge  ║
╠═════════╣    ╠════════════════════════════╣    ╠═══════════╣    ╠═════════╣
║ Publish ║ => ║ Retrieve => Aggregate      ║ => ║ Tally     ║ => ║ Deliver ║
╚═════════╝    ╠────────────────────────────╣    ╚═══════════╝    ╚═════════╝
               ║ Retrieve => Aggregate      ║
               ╠────────────────────────────╣
               ║ ... (as many as requested) ║
               ╚════════════════════════════╝
```

For the sake of deterministic execution, data flowing through the
different stages is strongly typed. The type of a value or data
structure defines the operations that can be done on the data.

For each stage, the data type of the input is the same as the data type
of the output of previous stage. Particularly, the aggregation and
tally stages gather multiple values or structures emitted by their
precedent stages, so they always receive an `Array`, i.e. if the
**retrieval** stage returned an `Integer`, the **aggregation** stage
will start with an `Array<Integer>`, that is, an array of `Integer`s.

For more information on data types, you can read the
[RADON documentation][radon], which provides a detailed description of
all the types and the operators they provide.

## The RAD Engine

The RAD Engine is the component in charge of processing data requests
coming from Witnet clients. That is, coordinating retrieval,
aggregation, tally and delivery of data strictly as specified in the
requests.

All data requests contain explicit instructions on what the RAD Engine
must do during every stage. These instructions, specified using
[__RAD Object Notation (RADON)__][radon], are interpreted by the RAD
Engine.

!!! info ""
    Just in case you were wondering, *RAD* stands for *Retrieve*,
    *Aggregate* and *Deliver*.

## RAD Object Notation (RADON)

The RAD Object Notation (RADON) is a declarative, functional, 
strongly-typed, Non-Turing complete programming language.

A RADON script is formed by a list of ordered calls (tuples of operator
byte codes and arguments) that are sequentially interpreted and applied
by the RAD Engine on the output of the previous call.

!!! example
    When applied on an `Array<Integer>`, this very simple 4-bytes RADON
    script will compute the average mean of all the `Integer`s:
    
    ```ts
    91 92 56 03
    ```
    
    ```ts
    [
        [ OP_ARRAY_REDUCE, REDUCER_AVG_MEAN ]   // [ 0x56, 0x03 ]
    ]
    ```
    
    Do not worry if you do not understand this script just yet. Keep on
    reading and then head yourself to the [RADON encoding][encoding]
    section for an explanation on how scripts are codified and
    formatted.

## Creating data requests

The RAD Engine is only capable of interpreting well-formed 
[RADON scripts][radon].

Even though human beings can safely write RADON without their heads
exploding, they are just not expected to. The **[Sheikah] desktop app**
is intended to be used as an IDE for visually and safely composing and
testing Witnet data requests.

It is also to be expected that at some point in the future, higher-level
programming languages may exist for writing data requests in a more 
expressive and user-friendly way.

[radon]: #rad-object-notation-radon
[encoding]: /protocol/data-requests/radon/encoding/
[sheikah]: https://github.com/witnet/sheikah