// Embeds the runtime CSVs brotli-compressed: as text they were ~5 MB of a ~17 MB desktop
// executable. Each CSV must match its manifest size and SHA-256 before it is embedded; the
// loader still decompresses and checks the exact original bytes at runtime.
use sha2::{Digest, Sha256};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const PROFILES: [(&str, &str); 2] = [
    ("vanilla", "../../data/phase1"),
    ("convergence", "../../data/profiles/convergence"),
];

const TABLES: [&str; 12] = [
    "aow.csv",
    "aow_attack_data.csv",
    "aow_effect_data.csv",
    "aow_route_assignments.csv",
    "attack_element_correct.csv",
    "attack_element_correct_ext.csv",
    "calc_correct.csv",
    "native_skill_attack_data.csv",
    "reinforce.csv",
    "weapon_passive_overlays.csv",
    "weapon_passives.csv",
    "weapons.csv",
];

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let params = brotli::enc::BrotliEncoderParams {
        quality: 9,
        lgwin: 22,
        ..Default::default()
    };
    let mut index = String::from("&[\n");
    for (profile, directory) in PROFILES {
        let manifest_path = Path::new(directory).join("manifest.json");
        println!("cargo:rerun-if-changed={}", manifest_path.display());
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(&manifest_path)
                .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display())),
        )
        .unwrap_or_else(|error| panic!("parse {}: {error}", manifest_path.display()));
        for table in TABLES {
            let source = Path::new(directory).join(table);
            println!("cargo:rerun-if-changed={}", source.display());
            let bytes = fs::read(&source)
                .unwrap_or_else(|error| panic!("read {}: {error}", source.display()));
            verify_against_manifest(&manifest, table, &bytes, &source);
            let mut compressed = Vec::new();
            brotli::BrotliCompress(&mut bytes.as_slice(), &mut compressed, &params)
                .unwrap_or_else(|error| panic!("compress {}: {error}", source.display()));
            let target = Path::new(&out_dir).join(format!("{profile}-{table}.br"));
            fs::write(&target, compressed).expect("write compressed table");
            writeln!(
                index,
                "    ({profile:?}, {table:?}, include_bytes!({:?}).as_slice()),",
                target.display().to_string()
            )
            .expect("format index");
        }
    }
    index.push(']');
    fs::write(Path::new(&out_dir).join("embedded_tables.rs"), index).expect("write index");
}

fn verify_against_manifest(manifest: &serde_json::Value, table: &str, bytes: &[u8], source: &Path) {
    let record = manifest["runtimeFiles"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["path"] == table))
        .unwrap_or_else(|| panic!("{} is not listed in its manifest", source.display()));
    let size = bytes.len() as u64;
    if record["size"].as_u64() != Some(size) {
        panic!(
            "{} is {size} bytes but its manifest says {}; regenerate the snapshot",
            source.display(),
            record["size"]
        );
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    if record["sha256"].as_str() != Some(digest.as_str()) {
        panic!(
            "{} has SHA-256 {digest} but its manifest says {}; regenerate the snapshot",
            source.display(),
            record["sha256"]
        );
    }
}
