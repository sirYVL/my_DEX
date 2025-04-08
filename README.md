# my_DEX - Global DEX Network (lightning-based trading network)

Einführung

MyDEX ist ein Open-Source-Projekt für eine dezentrale Austauschplattform (Decentralized Exchange, DEX) zum Handeln von Kryptowährungen. Im Gegensatz zu einer zentralisierten Börse ermöglicht eine DEX den direkten Handel zwischen Benutzer*innen, ohne dass eine vertrauenswürdige Zwischenstelle benötigt wird. Das bedeutet, dass Sie Ihre Coins direkt mit anderen tauschen können, während das System sicherstellt, dass alle Transaktionen korrekt und transparent ablaufen. Für die Nutzung oder Weiterentwicklung von MyDEX sind keine Vorkenntnisse in speziellen Blockchain-Frameworks (wie Hardhat) oder Frontend-Technologien notwendig – das System läuft als eigenständige Anwendung. 

Diese Plattform wurde in der Programmiersprache Rust entwickelt und legt besonderen Wert auf Vertrauensfreiheit (engl. trustlessness) und Sicherheit. MyDEX verwendet ein verteiltes Orderbuch – das ist eine Liste aller Kauf- und Verkaufsangebote, die von allen Knoten (Servern) im Netzwerk gemeinsam geführt wird. Durch diese verteilte Architektur gibt es keinen einzelnen Ausfallpunkt: Selbst wenn ein Knoten ausfällt, können die restlichen weiterarbeiten. Außerdem kann MyDEX mit mehreren Blockchains gleichzeitig interagieren. Dadurch können verschiedene Kryptowährungen über MyDEX gehandelt werden, ohne dass alle auf einer einzigen Blockchain liegen müssen. Zum Beispiel könnten Benutzer Bitcoin gegen Ethereum tauschen, wobei MyDEX die Transaktion koordiniert und sicherstellt, dass beide Seiten ihren Teil erhalten (oft mithilfe sogenannter Atomic Swaps, einer Technik für kettenübergreifende Trades). 

Projektstruktur

Dieses Repository ist in mehrere Ordner unterteilt, um die verschiedenen Komponenten des Projekts zu organisieren:

/src – Hauptcode von MyDEX (Orderbuch-Verwaltung, Matching Engine, Konsens-Mechanismen).

/dex-node – Ausführbares Rust-Programm („Node“), das einen MyDEX-Knoten startet.

/dex-cli – Kommandozeilenprogramm zur Interaktion mit MyDEX.

/dex-core – Rust-Bibliothek mit Kernfunktionen, nutzbar von dex-node und dex-cli.

/multi_asset_exchange – Beispielprogramm für Multi-Asset-Handel.

/config – Konfigurationsdateien (insb. node_config.yaml).

/docs – Technische Dokumentation und Spezifikationen.

/tests – Automatisierte Integrationstests.

/fuzz – Fuzzing-Tests für Robustheit und Sicherheit.

/benches – Benchmark-Tests zur Performance-Messung.

/grafana – Dashboard-Konfigurationen für Grafana.

/k8s – Kubernetes-Konfigurationsdateien (Helm Charts, Deployment-Dateien).

/monitoring – Dateien für Monitoring (Prometheus) und Alarmierung.

.github/workflows – Konfigurationen für CI/CD (Continuous Integration & Delivery).

Dockerfile und docker-compose.yaml – Docker-Unterstützung (Docker-Container).

Installation und Ausführung

Sie können MyDEX auf zwei Hauptwege auf Ihrem Rechner starten:

Option 1: Direkte Ausführung (mit Rust/Cargo)

Diese Methode eignet sich, wenn Sie die Entwicklungsversion ausprobieren oder am Code arbeiten möchten.

Rust installieren: Rust und Cargo über rustup installieren.

Projekt beziehen:

git clone <URL>
cd my_DEX

Konfiguration prüfen: Anpassungen in config/node_config.yaml vornehmen (optional).

Projekt kompilieren und starten:

cargo run --release

Zusätzliche Knoten starten (optional):

cargo run --release --config node_config2.yaml

Option 2: Ausführung mit Docker

Diese Methode ist praktisch, wenn keine Rust-Entwicklungsumgebung eingerichtet werden soll.

Docker-Image bauen:

docker build -t mydex .

Container starten:

docker run -it mydex

Konfiguration anpassen (optional):

docker run -d -v $(pwd)/config/node_config.yaml:/home/dexuser/node_config.yaml mydex

Mehrere Container starten (Docker Compose):

docker-compose up

Interaktion

Interaktion über Kommandozeilenanwendung (dex-cli):

cargo run --bin dex-cli -- <Befehl>

Ausblick

MyDEX befindet sich noch in der Entwicklung. Geplante Erweiterungen:

- Grafische Benutzeroberfläche

- Software-App für User die keinen Fullnode betreiben (Windows, Mac, Linux)

- Weitere Blockchain-Integration

- Performance & Skalierung

- Verbesserter Konsens und Sicherheit

- Erweiterte Dokumentation und Tutorials

MyDEX soll einfach, übersichtlich, selbsterklärend, intuitiv und benutzerfreundlich gestaltet sein. 

Beitrag leisten (internes Entwicklerteam)

Wir schätzen jede Mitarbeit innerhalb unseres Entwicklerteams! Wenn du Teil unseres Teams bist, halte dich bitte an diese Schritte:

Repository forken (intern)

Branch erstellen: feature/neue-funktion oder bugfix/issue-42

Änderungen vornehmen: Verständlich und wartbar entwickeln

Tests und Formatierung:

cargo fmt
cargo test

Pull Request stellen: Klar beschreiben, Feedback einholen und mergen lassen

Ideen frühzeitig diskutieren: Issues erstellen oder im Team besprechen
Es sollte noch einmal erwähnt werden, dass diese DEX absoluten Wert auf dezentralität, Sicherheit und Benutzerfreundlichkeit legt.


Gemeinsam verbessern und erweitern wir MyDEX stetig in unserem geschlossenen Entwicklerteam!
Es sollte noch einmal erwähnt werden, dass diese DEX absoluten Wert auf dezentralität legt.



   
