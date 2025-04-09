///////////////////////////////////////////////////////////
// my_dex/build.rs
///////////////////////////////////////////////////////////

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Wenn sich Dateien im Ordner "proto" �ndern, wird das Build-Skript neu ausgef�hrt.
    println!("cargo:rerun-if-changed=proto");

    // Konfiguriere und kompiliere die Protobuf-Dateien.
    // Wir gehen davon aus, dass "lightning_proto" ein Unterordner ist, der von den Protobuf-Dateien referenziert wird.
    tonic_build::configure()
        .build_server(true)
        .compile(
            &[
                "proto/chainkit.proto",
                "proto/invoices.proto",
                "proto/router.proto",
                "proto/signer.proto",
            ],
            &["proto", "proto/lightning_proto"],
        )?;
    Ok(())
}
