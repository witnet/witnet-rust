# Glossary

- __Conditional payment__: a Witnet transaction encumbered with a small program that defines how, when and by whom the enclosed value can be spent. Conditional payments can consume data coming from Witnet data requests, so they can be used to trigger release of funds upon the result of real world events without having to resort to more complex, stateful, turing-complete smart contracts.

- __Data request__: a digital document declaring one or more data sources and how data coming from those sources can be normalized and combined together in order to present it as a single data point to be consumed by other programs.

- __Decentralized network__: an overlay network in which multiple untrusted computers have been set to communicate with each other as peers using a network protocol, with the purpose of fulfilling some common utility, without any of them having prominent or absolute control over the network and without chance for anyone to disrupt the functioning of the network.

- __Oracle__: an entity providing smart contracts with information from outside their containing network. Tamper resistance is the main point of smart contracts, so they should only employ decentralized oracles in which they do not need to _trust the messenger_. Otherwise, the oracle entity would become a _single point of failure_. 

- __Smart contract__: a deterministic computer program with a high degree of resistance to tampering and censorship due to its concurrent execution by a decentralized network of processors owned by untrusted parties whose incentives deter them from colluding to alter the output of the program.