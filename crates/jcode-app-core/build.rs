use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Doc directories the bundled corpus skips: they describe intent or proposals,
/// not what the code does today.
const EXCLUDED_DIRS: &[&str] = &["plans", "proposals"];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo = manifest.join("../..");
    let docs_dir = repo.join("docs");
    println!(
        "cargo:rerun-if-changed={}",
        repo.join("README.md").display()
    );

    let mut files = vec![repo.join("README.md")];
    collect_docs(&docs_dir, &mut files);
    files.sort();

    let mut generated = String::from("pub(crate) static JCODE_DOCS: &[(&str, &str)] = &[\n");
    for path in files {
        let relative = path
            .strip_prefix(&repo)
            .expect("documentation is in repository");
        let relative = slash_path(relative);
        generated.push_str(&format!(
            "    ({relative:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../{relative}\"))),\n"
        ));
    }
    generated.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("jcode_docs.rs");
    fs::write(out, generated).expect("write generated Jcode documentation corpus");
}

/// Collect `*.md` under `dir`, recursing into subdirectories except the excluded
/// ones. Also registers each visited directory so a new file there reruns the build.
fn collect_docs(dir: &Path, out: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let excluded = entry
                .file_name()
                .to_str()
                .is_some_and(|name| EXCLUDED_DIRS.contains(&name));
            if !excluded {
                collect_docs(&path, out);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
