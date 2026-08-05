use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

fn main() {
    prepare_frontend_assets();
    tauri_build::build()
}

fn prepare_frontend_assets() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let project_root = manifest_dir
        .parent()
        .expect("Tauri manifest should be inside the project root");
    let frontend_sources = [
        project_root.join("src"),
        project_root.join("index.html"),
        project_root.join("toast.html"),
        project_root.join("vite.config.js"),
        project_root.join("package.json"),
        project_root.join("package-lock.json"),
    ];
    let frontend_dist = project_root.join("dist");

    for source in &frontend_sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }

    if !frontend_assets_are_current(&frontend_sources, &frontend_dist) {
        run_frontend_build(project_root);
    }
}

fn frontend_assets_are_current(sources: &[PathBuf], dist: &Path) -> bool {
    let dist_entry = dist.join("index.html");
    let Ok(dist_modified) = modified_at(&dist_entry) else {
        return false;
    };

    sources
        .iter()
        .filter_map(|path| latest_modified_at(path))
        .all(|source_modified| source_modified <= dist_modified)
}

fn latest_modified_at(path: &Path) -> Option<SystemTime> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.is_file() {
        return metadata.modified().ok();
    }

    let entries = fs::read_dir(path).ok()?;
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| latest_modified_at(&entry.path()))
        .max()
}

fn modified_at(path: &Path) -> Result<SystemTime, std::io::Error> {
    fs::metadata(path)?.modified()
}

fn run_frontend_build(project_root: &Path) {
    let npm_command = if cfg!(target_os = "windows") {
        "npm.cmd"
    } else {
        "npm"
    };
    let status = Command::new(npm_command)
        .args(["run", "build"])
        .current_dir(project_root)
        .status()
        .expect("failed to start the frontend build command");

    assert!(
        status.success(),
        "frontend build failed with status {status}"
    );
}
