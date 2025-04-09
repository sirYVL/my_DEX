// Folder: fuzz/fuzz_targets
// File: fuzz_nakamoto.rs

#![no_main]
use libfuzzer_sys::fuzz_target;

// Hier wird die Funktion validate_block aus der Nakamoto-Konsens-Logik importiert.
// Es wird angenommen, dass im Projekt unter src/consensus/nakamoto.rs diese Funktion existiert.
use my_dex::consensus::nakamoto::validate_block;

fuzz_target!(|data: &[u8]| {
    // Die Funktion validate_block wird mit beliebigen Eingabedaten aufgerufen.
    // Dadurch werden unerwartete Eingaben getestet, um potentielle Abst�rze oder Sicherheitsl�cken aufzudecken.
    let _ = validate_block(data);
});
