MyDEX – Global Dezentrale Exchange Netzwerk 

Überblick und Ziele

MyDEX (Name noch nicht festgelegt und kann sich ändern) ist eine dezentrale Exchange (DEX), die auf einer Layer-2-Architektur betrieben wird. Sie ermöglicht den Handel von Kryptowährungen über verschiedene Blockchains hinweg, ohne dass eine zentrale Instanz benötigt wird. Das System wurde in Rust entwickelt und legt besonderen Wert auf Trustlessness (Vertrauensfreiheit), Sicherheit und Modularität. MyDEX kombiniert eine Off-Chain-Orderbuch-Infrastruktur mit On-Chain-Abwicklung durch Atomic Swaps, um schnellen Handel und gleichzeitig sichere, unveränderliche Abrechnungen zu gewährleisten. Fullnodes spielen hierbei eine zentrale Rolle: Sie betreiben das Netzwerk, validieren alle Vorgänge und erhalten im Gegenzug einen Anteil der Handelsgebühren als dezentrale Belohnung.

Diese Dokumentation bietet einen technischen Überblick über die Architektur, den Technologie-Stack, die Hauptfunktionen und Module, die eingesetzten Smart Contracts sowie die Sicherheitsmechanismen von MyDEX. Außerdem werden die Anreizstruktur für Fullnode-Betreiber und Empfehlungen für Entwickler:innen gegeben, um einen leichten Einstieg in das Projekt zu ermöglichen.

Architekturübersicht

MyDEX ist als verteiltes System mit mehreren gleichberechtigten Fullnodes konzipiert. Jeder Fullnode führt sämtliche Kernkomponenten des DEX aus und kommuniziert mit anderen Knoten, um den Zustand der Exchange synchron zu halten. Wichtige Architekturmerkmale sind:

Verteilte Orderbuch-Architektur

Anstatt ein zentrales Orderbuch auf einer Blockchain zu führen, repliziert MyDEX das Orderbuch Off-Chain über alle Fullnodes mittels Gossip-Protokoll und konfliktfreier Datenstrukturen (CRDTs).

Off-Chain Matching, On-Chain Settlement

Die Order-Matching-Engine arbeitet Off-Chain, während die tatsächliche Abwicklung (Settlement) On-Chain erfolgt, insbesondere durch Atomic Swaps.

Modulare Komponenten

Die Software ist klar modularisiert:

Netzwerkschicht (Peer-to-Peer-Kommunikation, Kademlia DHT, Noise-Protokoll)

Orderbuch- und Handelslogik

Settlement-Engine (Atomic Swaps, Payment-Channels)

Konsens- und Validierungsschicht (Nakamoto, PBFT, Proof-of-Stake)

Speicher- und Datenbankschicht (RocksDB, IPFS)

Identitäts- und Zugriffsschicht (Benutzerkonten, Schlüsselverwaltung, HSM)

Monitoring und Selbstkontrolle (Prometheus, Heartbeats, Sicherheitschecks)

Off-Chain/On-Chain Kommunikation

Bitcoin und UTXO-Chains: Integration via Bitcoin Core RPC

Ethereum: Nutzung von Smart Contracts mit ethers-rs

Lightning Network: Payment-Channels via native Implementierung oder externe Lightning-Nodes

Node-Struktur & Rollen

Gatekeeper-Phase: Initialer Sicherheitscheck neuer Nodes

Komitee-Phase: Finales Voting für Node-Aufnahme

Fullnode-Status: Nach erfolgreichem Onboarding Gleichberechtigung aller Nodes

Technologie-Stack

MyDEX setzt auf Rust (Edition 2021), asynchrone Programmierung mit Tokio und verwendet umfangreich Rust-Crates wie Serde, Bincode, Tracing und Criterion.

Netzwerk & Kommunikation

Noise-Protokoll (verschlüsselte Kommunikation)

Kademlia DHT (Peer-Discovery)

Gossip-Protokoll

HTTP/REST-Schnittstelle (Axum)

Protobuf/gRPC

Blockchain-Integration

Unterstützte Blockchains und Tools:

Bitcoin (bitcoin crate)

Litecoin

Ethereum (ethers)

Monero (experimentell)

Lightning Network (native und externe Nodes)

Persistente Speicherung

RocksDB (lokale Speicherung)

IPFS (verteilte Speicherung)

Kryptographie & Sicherheit

SHA-2, Blake2, secp256k1, ed25519

Threshold-Signaturen

Ring Signatures, ZK-SNARKs

TLS/mTLS für externe Kommunikation

Infrastruktur & Deployment

Docker-Containerisierung

Kubernetes-Deployment-Templates

Grafana Dashboards für Monitoring

Funktionen und Module

Order-Verwaltung und Orderbuch

CRDT-basierte Orderbuch-Verwaltung

Order-Signaturen und Lebenszyklus

Matching Engine

Lokale, dezentrale Matching-Algorithmen

Konfliktlösungsmechanismen via Konsensus

Abwicklung und Atomic Swaps

HTLC-basierte Cross-Chain Swaps

Off-Chain Payment-Channels (Lightning)

Benutzerkonten und Wallets

Multi-Blockchain-Account-Verwaltung

Deposits, Withdrawals, MultiSig

HSM-Unterstützung

Gebührenmodell und Fee-Pool

Dezentrale Verteilung der Handelsgebühren

Pools für Entwickler und Nodes

Performance-basierte Node-Belohnung

Sicherheit und Trustless-Ansatz

Digitale Signaturen, Multi-Signaturen

Watchtowers, Monitoring und Sicherheitsmodule

Sybil-Resistenz, Node-Verifizierung, Staking

Passive Einnahmen für Fullnode-Betreiber

Leistungsorientierte Gebührenausschüttung

Dezentralisierte Governance und Transparenz

Einstieg für Entwickler:innen und Ausblick

Neue Entwickler:innen werden gezielt und strukturiert integriert:

Quellcode-Struktur verstehen (main.rs, Orderbuch, Matching-Engine)

Lokales Deployment via Docker

Kommunikation und Contribution via GitHub Workflows

Potentielle Entwicklungsfelder (UI, neue Assets, Sicherheit)

Diese Dokumentation richtet sich an technisch versierte Leser:innen. Für Endnutzer gibt es separate, vereinfachte Anleitungen.

Viel Erfolg beim Einarbeiten 
