use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::write::GzEncoder;
use flate2::Compression;
use tar::Builder;
use walkdir::WalkDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .expect("launcher crate must live inside the umoria repository");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    emit_rerun_if_changed(repo_root);

    let bundle_version = read_bundle_version(repo_root)?;
    println!("cargo:rustc-env=BUNDLE_VERSION={bundle_version}");

    let cmake_build = out_dir.join("cmake-build");
    fs::create_dir_all(&cmake_build)?;

    let cmake_status = Command::new("cmake")
        .arg(format!("-DCMAKE_BUILD_TYPE={}", profile_build_type()))
        .arg(repo_root)
        .current_dir(&cmake_build)
        .status()?;
    if !cmake_status.success() {
        panic!("cmake failed with status {cmake_status}");
    }

    let make_status = Command::new("make")
        .arg("-j")
        .current_dir(&cmake_build)
        .status()?;
    if !make_status.success() {
        panic!("make failed with status {make_status}");
    }

    let game_dir = cmake_build.join("umoria");
    let stage_dir = out_dir.join("bundle-stage");
    if stage_dir.exists() {
        fs::remove_dir_all(&stage_dir)?;
    }
    fs::create_dir_all(&stage_dir)?;

    copy_into(&game_dir.join("umoria"), &stage_dir.join("umoria"))?;
    copy_dir(&game_dir.join("data"), &stage_dir.join("data"))?;
    copy_into(&game_dir.join("AUTHORS"), &stage_dir.join("AUTHORS"))?;
    copy_into(&game_dir.join("LICENSE"), &stage_dir.join("LICENSE"))?;
    copy_into(&game_dir.join("scores.dat"), &stage_dir.join("scores.dat"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            stage_dir.join("umoria"),
            fs::Permissions::from_mode(0o755),
        )?;
    }

    let bundle_path = out_dir.join("bundle.tar.gz");
    write_bundle(&stage_dir, &bundle_path)?;

    Ok(())
}

fn profile_build_type() -> &'static str {
    if std::env::var("PROFILE").as_deref() == Ok("debug") {
        "Debug"
    } else {
        "Release"
    }
}

fn read_bundle_version(repo_root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let version_header = fs::read_to_string(repo_root.join("src/version.h"))?;
    let major = capture_version_part(&version_header, "CURRENT_VERSION_MAJOR")?;
    let minor = capture_version_part(&version_header, "CURRENT_VERSION_MINOR")?;
    let patch = capture_version_part(&version_header, "CURRENT_VERSION_PATCH")?;
    Ok(format!("{major}.{minor}.{patch}"))
}

fn capture_version_part(
    version_header: &str,
    field: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let marker = format!("{field} = ");
    let line = version_header
        .lines()
        .find(|line| line.contains(&marker))
        .ok_or_else(|| format!("missing {field} in src/version.h"))?;
    let value = line
        .split('=')
        .nth(1)
        .ok_or_else(|| format!("malformed {field} in src/version.h"))?
        .trim()
        .trim_end_matches(';')
        .trim();
    Ok(value.to_string())
}

fn emit_rerun_if_changed(repo_root: &Path) {
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("CMakeLists.txt").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("src/version.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("CHANGELOG.md").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("data").display()
    );

    for entry in WalkDir::new(repo_root.join("src"))
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
}

fn copy_into(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(destination)?;
    for entry in WalkDir::new(source).min_depth(1) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn write_bundle(stage_dir: &Path, bundle_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bundle_file = File::create(bundle_path)?;
    let encoder = GzEncoder::new(bundle_file, Compression::default());
    let mut archive = Builder::new(encoder);

    for entry in WalkDir::new(stage_dir).min_depth(1) {
        let entry = entry?;
        let relative = entry
            .path()
            .strip_prefix(stage_dir)?
            .to_string_lossy()
            .replace('\\', "/");
        if entry.file_type().is_dir() {
            archive.append_dir(relative, entry.path())?;
        } else {
            archive.append_path_with_name(entry.path(), relative)?;
        }
    }

    let encoder = archive.into_inner()?;
    let mut bundle_file = encoder.finish()?;
    bundle_file.flush()?;
    Ok(())
}
