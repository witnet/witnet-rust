# What is Witnet

The Witnet protocol, as outlined by the [Witnet Whitepaper][whitepaper], allows a network of computers to act as a "decentralized oracle" that retrieves, attests and delivers information to smart contracts without having to place trust in a single entity.

Wait, what? Ok, lets go one step at a time.

## Smart Contracts Are Not What You Think

Over the last years, blockchain technology has promised to revolutionize business by allowing creation of "smart contracts" that, unlike paper contracts, are impossible to breach.

Actually, those smart contracts are nothing more than small programs that can be run in a trustless manner.
That is: once they are created, no one can stop them from doing exactly what they were created for.
They just obey their own source code, and censorship is just impossible.

This is a really powerful idea. If you can write a smart contract that:

1. implements the logic of an agreement, and
2. can execute the clauses of the contract on its own (like paying Alice or Bob depending on the outcome of some event),

then you have a contract that is capable of enforcing itself and leaves no room for contestation. Boom :bomb:.

## Blockchain Oracles, And Their Problem

Given that smart contracts need to be completely deterministic[^1], they do not support input of data from non-deterministic sources such as APIs or websites.

As a result, smart contracts are mostly isolated from the rest of the Internet, which dramatically reduces their transformative potential.
At the end of the day, the output of a program does not depend solely on its source code, but also on the input data it operates upon.

Of course, as the creator of a smart contract, you can create a method that allows you and only you to act as an "oracle" by introducing information from the outside at will.
But you would be completely breaking the trustless nature of a smart contract.
If trust is put in a single entity, there you have a single point of failure that can easily be hacked or corrupted.  

Smart contracts connected to the real world will not be completely trustless and will not release their full potential until we have ways to feed them information trustlessly.

This is often called _"the oracle problem"_. 

## The Solution: A Decentralized Oracle Network

The Witnet protocol aims to create an overlay network that connects smart contracts to any online data source.
Sport results, stock prices, weather forecasts or even other blockchains can be easily queried (preferably through APIs).

The protocol describes a distributed network of peer nodes—which we fondly call _witnesses_—who earn Wit tokens as a reward for retrieving web data and reporting it directly to the smart contracts.

The bottom line is that a considerable number of randomly selected, anonymous peers retrieving information from one or more sources can converge into a single truth about the data they retrieved if a majority of them are incentivized to report the retrieved data honestly and they apply a common consensus algorithm that resolves inconsistencies.

This Decentralized Oracle Network (DON) maintains and distributes a block chain data structure that serves as a common ledger for the operation of the protocol as well as for the wit token, which is central to incentivizing the network players to abide by the protocol and make them liable for any misbehavior.
Witnesses are also in charge of validating transactions in the network and bundling them into blocks that get appended to the blockchain periodically.

The process by which witnesses retrieve, attest and deliver data in behalf of the smart contracts is in some way similar to mining in other blockchains.
However, fulfilling these tasks and collecting the rewards is not that expensive in terms of computation.

The protocol has been conceived to ensure utmost decentralization and fairnes, so each witness' weight in the network is not aligned to their computing power.
Instead, the probability for every witness to be assigned tasks or mine new blocks is directly proportional to their past performance in terms of honesty: their reputation. 

!!! tip
    Of course, the so-called miners are not actual human beings sitting in front of a computer, fulfilling assignments coming from an Internet overlord that commands them to use their web browser to navigate to a certain website and take a snapshot or copy some text that they must report.
    Indeed, the miners are just computers running a software (Witnet-rust) that automatically receive and execute a series of tasks without the owner of the computer having to actively do anything else than installing it.

## 100% Truth, 0% Trust

Data retrieved, attested and delivered using the Witnet protocol is reliable not because of authority but because it comes from anonymous nodes who are incentivized to remain honest and to compete for rewards.

In addition, integrity of this data is guaranteed by a consensus algorithm that detects fraudsters, who are immediately punished.

The progressive [reputation protocol] plays a central role in maintaining every participant active and honest by creating short, middle and long term incentives for them to abide by the protocol and not to tamper with the data they broker. 

!!! info
    Please note that Witnet's aim is not spotting fake data, but guaranteeing a 1:1 match between what is published online—regardless of its truthness—and the data that is eventually delivered the smart contracts.

## Who Is Behind Witnet

Witnet is an open source project originally devised by [Stampery], the leaders of blockchain-powered data certification.
The protocol is now being developed by [Witnet Foundation] in collaboration with a community of independent contributors.

Ever since Stampery was founded in 2014, they have been on a mission: replacing blind trust with mathematical proof.
Witnet is the next step towards this goal.

Stampery is backed by top venture capital funds and angel investors, including Tim Draper and Blockchain Capital.

The Stampery team has also been involved in the development of [Aragon], [Trailbot], [Mongoaudit] and [Loqui IM].

[^1]: Otherwise, the contracts could have totally different output values when executed across all the nodes maintaining the blockchain, therefore causing inconsistencies that would lead to breaking the network consensus.

[whitepaper]: https://witnet.io/static/witnet-whitepaper.pdf
[reputation protocol]: protocol/reputation.md
[Stampery]: https://stampery.com
[Witnet Foundation]: https://witnet.foundation
[Aragon]: https://aragon.org
[Trailbot]: https://trailbot.io
[Mongoaudit]: https://mongoaud.it
[Loqui IM]: https://loqui.im