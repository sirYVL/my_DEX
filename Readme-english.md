my_DEX - Global DEX Network (lightning-based trading network)
 
A Decentralized Exchange (DEX) for Cryptocurrencies

Introduction

MyDEX is an open-source project for a decentralized exchange platform (DEX) for trading cryptocurrencies. Unlike a centralized exchange, a DEX enables direct trading between users without requiring a trusted intermediary. This means you can directly trade your coins with others, while the system ensures that all transactions occur correctly and transparently. No prior knowledge of specific blockchain frameworks (such as Hardhat) or frontend technologies is required for using or developing MyDEX—the system operates as a standalone application. The platform is developed in Rust and places special emphasis on trustlessness and security. MyDEX uses a distributed order book—a list of all buy and sell offers maintained collectively by all nodes (servers) in the network. Due to this distributed architecture, there is no single point of failure: even if one node fails, the rest can continue operating. Additionally, MyDEX can interact with multiple blockchains simultaneously. This enables trading various cryptocurrencies via MyDEX without all needing to reside on a single blockchain. For example, users can trade Bitcoin for Ethereum, with MyDEX coordinating the transaction to ensure both parties receive their part (often using so-called Atomic Swaps, a technique for cross-chain trades).

Project Structure

This repository is organized into several folders to structure the project's various components:

/src – Main MyDEX code (order book management, matching engine, consensus mechanisms).

/dex-node – Executable Rust program ("Node") that launches a MyDEX node.

/dex-cli – Command-line program for interacting with MyDEX.

/dex-core – Rust library containing core functions, usable by dex-node and dex-cli.

/multi_asset_exchange – Sample program demonstrating multi-asset trading.

/config – Configuration files (especially node_config.yaml).

/docs – Technical documentation and specifications.

/tests – Automated integration tests.

/fuzz – Fuzzing tests for robustness and security.

/benches – Benchmark tests for performance measurement.

/grafana – Dashboard configurations for Grafana.

/k8s – Kubernetes configuration files (Helm charts, deployment files).

/monitoring – Files for monitoring (Prometheus) and alerting.

.github/workflows – Configurations for CI/CD (Continuous Integration & Delivery).

Dockerfile and docker-compose.yaml – Docker support (Docker containers).

Installation and Execution

You can start MyDEX on your computer using two main methods:

Option 1: Direct Execution (with Rust/Cargo)

This method is suitable if you want to test the development version or work directly with the code.

Install Rust: Install Rust and Cargo via rustup.

Obtain the project:

git clone <URL>
cd my_DEX

Check configuration: Modify config/node_config.yaml as necessary (optional).

Compile and start the project:

cargo run --release

Start additional nodes (optional):

cargo run --release --config node_config2.yaml

Option 2: Execution with Docker

This method is practical if you prefer not to set up a Rust development environment.

Build Docker image:

docker build -t mydex .

Start container:

docker run -it mydex

Adjust configuration (optional):

docker run -d -v $(pwd)/config/node_config.yaml:/home/dexuser/node_config.yaml mydex

Start multiple containers (Docker Compose):

docker-compose up

Interaction

Interact via command-line application (dex-cli):

cargo run --bin dex-cli -- <command>

Future Plans

MyDEX is still under development. Planned expansions include:

Graphical user interface

Additional blockchain integrations

Performance & scalability improvements

Enhanced consensus and security mechanisms

Expanded documentation and tutorials

MyDEX aims to be an open project that continually grows through community feedback and contributions.

Contribution (Internal Development Team)

We appreciate contributions within our development team! If you're part of our team, please follow these steps:

Fork repository (internally)

Create branch: feature/new-function or bugfix/issue-42

Make changes: Develop clearly and maintainably

Testing and formatting:

cargo fmt
cargo test

Submit pull request: Clearly describe, solicit feedback, and await merge

Discuss ideas early: Create issues or discuss within the team

Together, we continuously improve and expand MyDEX within our dedicated developer team!

