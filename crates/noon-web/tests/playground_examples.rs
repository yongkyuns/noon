use noon_web::ScenePlayer;
use std::{fs, path::PathBuf, process::Command};

fn generate_playground_scenes(extra_args: &[&str]) -> (PathBuf, String) {
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output_dir = std::env::temp_dir().join(format!(
        "noon-playground-scenes-{}-{}",
        std::process::id(),
        extra_args.join("-").replace("--", "")
    ));
    let _ = fs::remove_dir_all(&output_dir);
    fs::create_dir_all(&output_dir).expect("temporary playground scene directory is writable");

    let output = Command::new("python3")
        .arg(repository_root.join("web/python/playground_examples.py"))
        .arg(&output_dir)
        .args(extra_args)
        .current_dir(&repository_root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        // Exercise the manifest producer under a hostile inherited encoding.
        // Its machine-readable stdout contract must remain UTF-8 on every host.
        .env("PYTHONIOENCODING", "cp1252")
        .output()
        .expect("python3 is available for playground validation");

    if !output.status.success() {
        panic!(
            "playground Python execution failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let manifest = String::from_utf8(output.stdout).expect("playground manifest is UTF-8");
    (output_dir, manifest)
}

fn assert_manifest_compiles(output_dir: &PathBuf, manifest: &str) {
    let registered = manifest
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let mut compiled = 0usize;
    let mut failures = Vec::new();
    for line in manifest.lines().filter(|line| !line.trim().is_empty()) {
        let (name, document_path) = line
            .split_once('\t')
            .expect("playground manifest line contains a name and JSON path");
        match fs::read_to_string(document_path) {
            Ok(document) => match ScenePlayer::from_scene_json(&document) {
                Ok(_) => compiled += 1,
                Err(error) => failures.push(format!("{name}: {error}")),
            },
            Err(error) => failures.push(format!("{name}: failed to read document: {error}")),
        }
    }

    assert!(registered > 0, "playground registers at least one scene");
    assert!(
        failures.is_empty(),
        "playground scenes failed to compile:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        compiled, registered,
        "every registered playground scene was compiled"
    );
    fs::remove_dir_all(output_dir).expect("temporary playground scene directory is removable");
}

#[test]
fn every_playground_scene_executes_and_compiles() {
    let (output_dir, manifest) = generate_playground_scenes(&[]);
    assert_manifest_compiles(&output_dir, &manifest);
}

#[test]
fn compact_morph_stress_scene_executes_and_compiles() {
    let (output_dir, manifest) =
        generate_playground_scenes(&["--morph-stress-count", "96"]);
    assert!(
        manifest.contains("Morph stress · 1,000"),
        "compact generation keeps the canonical gallery corpus"
    );
    assert_manifest_compiles(&output_dir, &manifest);
}
