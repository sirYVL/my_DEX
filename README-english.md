MyDEX – Global Decentralized Exchange Network

Overview and Objectives

MyDEX (name not finalized and subject to change) is a decentralized exchange (DEX) operating on a Layer-2 architecture. It enables cryptocurrency trading across multiple blockchains without the need for a central authority. Built in Rust, the system prioritizes trustlessness, security, and modularity. MyDEX combines an off-chain order book infrastructure with on-chain settlements via Atomic Swaps, ensuring rapid trading and secure, immutable transactions. Fullnodes play a crucial role by operating the network, validating all transactions, and receiving a share of the trading fees as decentralized rewards.

This documentation provides a technical overview of MyDEX's architecture, technology stack, core functionalities and modules, smart contracts in use, and security mechanisms. Additionally, it outlines the incentive structure for fullnode operators and recommendations for developers to facilitate easy onboarding into the project.

Architectural Overview

MyDEX is designed as a distributed system consisting of multiple peer-to-peer fullnodes. Each fullnode executes all core components of the DEX and communicates with other nodes to maintain a synchronized state of the exchange. Key architectural features include:

Distributed Order Book Architecture

Instead of maintaining a centralized blockchain-based order book, MyDEX replicates the order book off-chain across all fullnodes using gossip protocols and conflict-free replicated data types (CRDTs).

Off-Chain Matching, On-Chain Settlement

The order matching engine operates off-chain, whereas the actual trade settlement is executed on-chain, primarily through Atomic Swaps.

Modular Components

The software is clearly modularized into:

Network Layer (peer-to-peer communication, Kademlia DHT, Noise protocol)

Order Book and Trading Logic

Settlement Engine (Atomic Swaps, payment channels)

Consensus and Validation Layer (Nakamoto, PBFT, Proof-of-Stake)

Storage and Database Layer (RocksDB, IPFS)

Identity and Access Layer (user accounts, key management, HSM integration)

Monitoring and Self-Regulation (Prometheus, heartbeats, security checks)

Off-Chain/On-Chain Communication

Bitcoin and UTXO chains: Integration via Bitcoin Core RPC

Ethereum: Smart contracts via ethers-rs

Lightning Network: Payment channels via native implementation or external Lightning nodes

Node Structure & Roles

Gatekeeper Phase: Initial security checks for new nodes

Committee Phase: Final voting for node acceptance

Fullnode Status: Equal rights for all nodes after successful onboarding

Technology Stack

MyDEX leverages Rust (Edition 2021), asynchronous programming with Tokio, and extensively uses Rust crates such as Serde, Bincode, Tracing, and Criterion.

Network & Communication

Noise protocol (encrypted communication)

Kademlia DHT (peer discovery)

Gossip protocol

HTTP/REST interface (Axum)

Protobuf/gRPC

Blockchain Integration

Supported blockchains and tools:

Bitcoin (bitcoin crate)

Litecoin

Ethereum (ethers)

Monero (experimental)

Lightning Network (native and external nodes)

Persistent Storage

RocksDB (local storage)

IPFS (distributed storage)

Cryptography & Security

SHA-2, Blake2, secp256k1, ed25519

Threshold signatures

Ring Signatures, ZK-SNARKs

TLS/mTLS for external communication

Infrastructure & Deployment

Docker containerization

Kubernetes deployment templates

Grafana dashboards for monitoring

Functions and Modules

Order Management and Order Book

CRDT-based order book management

Order signatures and lifecycle management

Matching Engine

Local, decentralized matching algorithms

Conflict resolution via consensus mechanisms

Settlement and Atomic Swaps

HTLC-based cross-chain swaps

Off-chain payment channels (Lightning)

User Accounts and Wallets

Multi-blockchain account management

Deposits, withdrawals, multi-signature

HSM support

Fee Model and Fee Pool

Decentralized distribution of trading fees

Pools for developers and nodes

Performance-based node rewards

Security and Trustless Approach

Digital signatures, multi-signature schemes

Watchtowers, monitoring, and security modules

Sybil resistance, node verification, staking

Passive Income for Fullnode Operators

Performance-oriented fee distribution

Decentralized governance and transparency

Developer Onboarding and Outlook

New developers are carefully selected and integrated through a structured approach:

Understanding source code structure (main.rs, order book, matching engine)

Local deployment via Docker

Communication and contributions via GitHub workflows

Potential development areas (UI, new assets, security)

This documentation is intended for technically proficient readers. Simplified user guides will be provided separately.

Good luck onboarding
