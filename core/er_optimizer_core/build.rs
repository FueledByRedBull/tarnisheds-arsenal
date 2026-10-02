// Embeds the runtime CSVs brotli-compressed: as text they were ~5 MB of a ~17 MB desktop
// executable. The loader decompresses them and then checks the manifest's sizes and hashes
// against the exact original bytes, as before.
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
    "aow_route_assignments.csv",
    "aow_effect_data.csv",
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
        for table in TABLES {
            let source = Path::new(directory).join(table);
            println!("cargo:rerun-if-changed={}", source.display());
            let bytes = fs::read(&source)
                .unwrap_or_else(|error| panic!("read {}: {error}", source.display()));
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
