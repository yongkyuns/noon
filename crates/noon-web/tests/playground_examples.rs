use noon_web::ScenePlayer;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn every_playground_scene_executes_and_compiles() {
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output_dir =
        std::env::temp_dir().join(format!("noon-playground-scenes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&output_dir);
    fs::create_dir_all(&output_dir).expect("temporary playground scene directory is writable");

    let output = Command::new("python3")
        .arg(repository_root.join("web/python/playground_examples.py"))
        .arg(&output_dir)
        .current_dir(&repository_root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("python3 is available for playground validation");

    if !output.status.success() {
        panic!(
            "playground Python execution failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let manifest = String::from_utf8(output.stdout).expect("playground manifest is UTF-8");
    let mut compiled = 0usize;
    for line in manifest.lines().filter(|line| !line.trim().is_empty()) {
        let (name, document_path) = line
            .split_once('\t')
            .expect("playground manifest line contains a name and JSON path");
        let document = fs::read_to_string(document_path)
            .unwrap_or_else(|error| panic!("failed to read {name} document: {error}"));
        ScenePlayer::from_scene_json(&document)
            .unwrap_or_else(|error| panic!("playground scene {name:?} failed to compile: {error}"));
        compiled += 1;
    }

    assert_eq!(
        compiled, 13,
        "every registered playground scene was compiled"
    );
    fs::remove_dir_all(&output_dir).expect("temporary playground scene directory is removable");
}
