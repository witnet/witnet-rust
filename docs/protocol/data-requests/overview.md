# Data requests

Data requests are the cornerstone of the Witnet protocol. They allow **clients** to have **witness** nodes **retrieve**,
**aggregate** and **deliver** data on their behalf on demand.

## Request life cycle

Once a data request has been published by a client, it will go through 4 distinct phases: ***retrieval***, 
***aggregation***, ***consensus*** and ***delivery***.
These phases are linear and constitute a single, unidirectional data flow.

```
╔═════════╗    ╔════════════════════════════╗    ╔═══════════╗    ╔═════════╗
║ Client  ║    ║ Witnesses                  ║    ║ Miner     ║    ║ Bridge  ║
╠═════════╣    ╠════════════════════════════╣    ╠═══════════╣    ╠═════════╣
║ Publish ║ => ║ Retrieve => Aggregate      ║ => ║ Consensus ║ => ║ Deliver ║
╚═════════╝    ╠────────────────────────────╣    ╚═══════════╝    ╚═════════╝
               ║ Retrieve => Aggregate      ║
               ╠────────────────────────────╣
               ║ ... (as many as requested) ║
               ╚════════════════════════════╝
```

In each phase, its input data is the output data of the previous phase.

For the sake of deterministic execution, data flowing through the different phases is strongly typed. The type of a
value or data structure defines the operations that can be done on the data.

In each phase, its input data type is the output data type of the previous phase. Particularly, the aggregation and
consensus phases gather multiple values or structures emitted by their precedent phases, so they always receive an
`Array`.

For more information on data types, you can read the [RADON documentation][radon], which provides a detailed description
of all the types and the operators they provide.

## The RAD Engine

The RAD Engine is the component in charge of processing data requests coming from Witnet clients.
That is, coordinating retrieval, aggregation, consensus and delivery of data strictly as specified in the requests.

All data requests contain explicit instructions on what the RAD Engine must do during every phase.
These instructions, specified using [__RAD Object Notation (RADON)__][radon], are interpreted by the RAD Engine.

!!! info ""
    Just in case you were asking, *RAD* stands for *Retrieve*, *Aggregate* and *Deliver*.

## RAD Object Notation (RADON)

The RAD Object Notation (RADON) is a low-level, declarative, functional, strongly-typed, Non-Turing complete programming language.

A RADON script is formed by a list of ordered calls (tuples of operators and arguments) that are sequentially
interpreted and applied by the RAD Engine on the output of the previous call.

## Creating data requests

The RAD Engine is only capable of interpreting well-formed [RADON scripts][radon].

Even though human beings can safely write RADON without their heads exploding 🤯, they are just expected to do not.
The higher-level **[RADlang][radlang]** programming language should be used instead for writing data requests in
a much more expressive and user-friendly way.

The **[Sheikah] desktop app** is intended to be used as an IDE for Witnet data requests, so it will act as a compiler for
transforming RADlang into RADON.

While RADlang and Sheikah are maintained by Witnet Foundation, other third-party developers can create their own
high-level programming languages to abstract away from the complexity of RADON.

[radon]: #rad-object-notation-radon
[radlang]: ../radlang
[sheikah]: https://github.com/witnet/sheikah