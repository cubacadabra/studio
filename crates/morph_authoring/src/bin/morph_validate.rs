//! Validate a `.morph.json` sidecar against a GLB export.

use cubacadabra_morph_authoring::{inspect_glb_bytes, parse_source_manifest};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 2 {
        eprintln!("usage: morph_validate <manifest.morph.json> <asset.glb>");
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
    match inspect_glb_bytes(&manifest, &glb) {
        Ok(inspection) => {
            println!(
                "valid {}: GLB v{} ({} JSON bytes, {} BIN bytes)",
                manifest.asset.id, inspection.version, inspection.json_bytes, inspection.bin_bytes
            );
            for (level, lod) in inspection.lods {
                println!(
                    "  {level}: node={} mesh={} primitives={} triangles={}",
                    lod.node, lod.mesh_index, lod.primitive_count, lod.triangle_count
                );
            }
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            print_diagnostics(&diagnostics);
            ExitCode::from(1)
        }
    }
}

fn print_diagnostics(diagnostics: &[cubacadabra_morphs::MorphDiagnostic]) {
    for diagnostic in diagnostics {
        eprintln!(
            "{} {}: {}",
            diagnostic.code, diagnostic.path, diagnostic.message
        );
    }
}
