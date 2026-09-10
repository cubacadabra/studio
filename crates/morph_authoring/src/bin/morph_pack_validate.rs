//! Validate a compiled `.morphpack` with the shared runtime decoder.

use cubacadabra_morphs::decode_morph_pack;
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 1 {
        eprintln!("usage: morph_pack_validate <asset.morphpack>");
        return ExitCode::from(2);
    }
    let bytes = match fs::read(&arguments[0]) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {}: {error}", arguments[0]);
            return ExitCode::from(2);
        }
    };
    let pack = match decode_morph_pack(&bytes) {
        Ok(pack) => pack,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!(
                    "{} {}: {}",
                    diagnostic.code, diagnostic.path, diagnostic.message
                );
            }
            return ExitCode::from(1);
        }
    };
    println!(
        "valid {}: {} bytes (near {}, mid {}, far {} triangles)",
        pack.asset.id,
        bytes.len(),
        pack.lods[0].triangle_count,
        pack.lods[1].triangle_count,
        pack.lods[2].triangle_count
    );
    ExitCode::SUCCESS
}
