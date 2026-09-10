//! Compile a validated `.morph.json` sidecar and GLB into a `.morphpack`.

use cubacadabra_morph_authoring::{compile_morph_pack, parse_source_manifest};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        eprintln!("usage: morph_compile <manifest.morph.json> <asset.glb> <output.morphpack>");
        return ExitCode::from(2);
    }

    let manifest_source = match fs::read_to_string(&arguments[0]) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("cannot read {}: {error}", arguments[0]);
            return ExitCode::from(2);
        }
    };
    let manifest = match parse_source_manifest(&manifest_source) {
        Ok(manifest) => manifest,
        Err(diagnostics) => {
            print_diagnostics(&diagnostics);
            return ExitCode::from(1);
        }
    };
    let glb = match fs::read(&arguments[1]) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {}: {error}", arguments[1]);
            return ExitCode::from(2);
        }
    };
    let (pack, summary) = match compile_morph_pack(&manifest, &glb) {
        Ok(result) => result,
        Err(diagnostics) => {
            print_diagnostics(&diagnostics);
            return ExitCode::from(1);
        }
    };
    if let Err(error) = fs::write(&arguments[2], &pack) {
        eprintln!("cannot write {}: {error}", arguments[2]);
        return ExitCode::from(2);
    }
    println!(
        "compiled {}: {} bytes ({:?} triangles)",
        summary.asset_id, summary.byte_len, summary.lod_triangle_counts
    );
    ExitCode::SUCCESS
}

fn print_diagnostics(diagnostics: &[cubacadabra_morphs::MorphDiagnostic]) {
    for diagnostic in diagnostics {
        eprintln!(
            "{} {}: {}",
            diagnostic.code, diagnostic.path, diagnostic.message
        );
    }
}
