# my_dex/Dockerfile
#
# NEU (Sicherheitsupdate):
# 1) Wir erstellen im Runtime-Stage einen dedizierten Benutzer "dex" und starten nicht als root.
# 2) Zusätzliche Kommentare zu Minimalismus & schreibgeschütztem Dateisystem.
#

# 1) Builder-Stage: Erstelle das Release-Binary
FROM rust:1.69 as builder

WORKDIR /usr/src/my_dex

# Kopiere zunächst die Cargo-Konfigurationsdateien, um Abhängigkeiten zu cachen
COPY Cargo.toml Cargo.lock ./

# Erstelle einen leeren src-Ordner mit einem Dummy-Hauptprogramm (wichtig für den Cache)
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Kopiere nun den gesamten Quellcode
COPY . ./

# Baue das Release-Binary
RUN cargo build --release

# 2) Runtime-Stage: Verwende ein schlankes Linux-Image
FROM debian:bullseye-slim

# Installiere notwendige Pakete (z. B. CA-Zertifikate)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# NEU: Erstelle einen dedizierten Benutzer "dex"
#      und führe den Container nachher als non-root.
RUN useradd -m -s /bin/bash dex

WORKDIR /home/dex

# Kopiere das gebaute Binary aus der Builder-Stage
COPY --from=builder /usr/src/my_dex/target/release/my_dex /usr/local/bin/my_dex

# Kopiere optional den Konfigurationsordner (falls benötigt)
COPY config ./config

# Setze Umgebungsvariablen, z. B. für das Logging
ENV RUST_LOG=info

# Exponiere die notwendigen Ports
EXPOSE 9000 9100

# Wechsle zum nichtprivilegierten Benutzer
USER dex

# (Optional) In Production kannst du das Filesystem read-only machen:
# 1) "RUN chown -R dex:dex /home/dex"
# 2) "VOLUME /home/dex/data" (wenn du Logs oder DB brauchst)
# 3) "CMD [\"my_dex\", \"--some\", \"args\"]" z. B. Args

# Setze den Container-Eintrittspunkt
ENTRYPOINT ["/usr/local/bin/my_dex"]
