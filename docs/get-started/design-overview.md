The Witnet protocol aims to guarantee properties common to all public blockchains such as safety, liveness and fault tolerance. Yet, there are specific design goals and decisions that make Witnet different from other solutions.

## Data integrity

In oracle solutions, tamper-resistance and censor-resistance boil down to _data integrity_—ensuring data is brokered from the sources to the consumers without manipulation.

The Witnet protocol is designed to preserve data integrity by solving data requests through:

- __Crowd-witnessing__: the final result of a data request is not given by an individual node but is the result of a consensus achieved by several data retrievals made by a randomly selected group of nodes.
- __Multi-source fact checking__: the protocol offers the possibility to query many data sources and aggregate their responses before reaching a consensus.
- __Economic incentives__: honest nodes are rewarded while dishonest ones are penalized.
- __Deterring conflicts of interest__: provided by clear segregation of duties among all the actors involved in data request life cycles.

## Fairness

Beyond the economic and ethical implications of making the Witnet ecosystem as accessible as possible to everyone, any technical measure ensuring "fairness" in the protocol has very practical implications.
Fairness seeks to guarantee that the network is not controlled by a few actors that may collude to tamper with the data requests.
This fairness is achieved through:

- __Low barriers to entry for new nodes__, which means that new nodes do not need to stake a significant monetary amount nor invest in expensive hardware to become eligible to resolve data requests or mining. Nodes will compete with each other with respect to their reputation, which can be easily gained by behaving honestly (not tampering with the data).
- __Anti-hoarding measures__. Spurious incentives may appear if Witnet allowed a perpetuation in the power of the most reputed nodes. These could be bribed to lie in specific data requests for which they could prove valid eligibility. Further, and very related to our previous point, we would like Witnet to allow anyone to potentially become a reputed node.

## Radical parametrization

A decentralized oracle solution should be parameterizable enough to enable as many use cases as possible, which may require different types of setups, incentives and trade-offs.

Some examples about this parametrization are:

- __Tailored data requests retrieval and aggregation__: clients define programatically how the results of one or multiple sources are retrieved, aggregated and verified.
- __Tailored data request consensus__: additionally, clients also include the tally clauses in order to programmatically define how to turn the multiple data request results provided by witnesses into a single data point.
- __Parameterizable incentives__: honest resolution of data requests is incentivized by using several mechanisms which can be configured and fine tuned to achieve a specific degree of certainty and security.

Some examples for these customizable incentives are:

- __Data request reward__: incentivizes the honest behavior of the nodes resolving the data request (a.k.a. the witnesses)
- __Number of witnesses__: how many nodes we want the data request to be executed by
- __Network fees__: incentives to the network to include the transactions that are required for the data request lifecycle into subsequent blocks
- __Collateral and coin age__: guarantee the neutrality and honesty of data request committees (prevents sybil attacks)

Ironically, even if all of these customizable incentive mechanisms contribute to make Witnet an exceptionally robust design, one of the main virtues of this system lays indeed in what is not parameterizable: the committee selection.
In Witnet, committee members (the nodes in charge of resolving a specific data request) are selected unpredictably through Verifiable Random Functions (VRF) weighted by the participants’ reputation scores. That is, the more reputed your node is, the more committees it will end up sitting on.