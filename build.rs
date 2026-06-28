use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::Archive;
use tar::Builder;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct GameSource {
    repo: String,
    tag: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let game_source = read_game_source(&manifest_dir)?;

    println!("cargo:rerun-if-changed={}", manifest_dir.join("build.rs").display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("game-source.toml").display()
    );

    let game_root = resolve_game_root(&out_dir, &game_source)?;
    let bundle_version = read_bundle_version(&game_root)?;
    println!("cargo:rustc-env=BUNDLE_VERSION={bundle_version}");

    let cmake_build = out_dir.join("cmake-build");
    if cmake_build.exists() {
        fs::remove_dir_all(&cmake_build)?;
    }
    fs::create_dir_all(&cmake_build)?;

    let cmake_status = Command::new("cmake")
        .arg(format!("-DCMAKE_BUILD_TYPE={}", profile_build_type()))
        .arg(&game_root)
        .env("CXXFLAGS", "-w")
        .current_dir(&cmake_build)
        .status()?;
    if !cmake_status.success() {
        panic!("cmake failed with status {cmake_status}");
    }

    let make_status = Command::new("make")
        .arg("-j")
        .env("CXXFLAGS", "-w")
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

    write_bundle(&stage_dir, &out_dir.join("bundle.tar.gz"))?;

    Ok(())
}

fn read_game_source(manifest_dir: &Path) -> Result<GameSource, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(manifest_dir.join("game-source.toml"))?;
    let mut repo = None;
    let mut tag = None;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key.trim() {
            "repo" => repo = Some(value.trim().trim_matches('"').to_string()),
            "tag" => tag = Some(value.trim().trim_matches('"').to_string()),
            _ => {}
        }
    }

    Ok(GameSource {
        repo: repo.ok_or("game-source.toml is missing repo")?,
        tag: tag.ok_or("game-source.toml is missing tag")?,
    })
}

fn resolve_game_root(
    out_dir: &Path,
    game_source: &GameSource,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(local_source) = std::env::var("UMORIA_GAME_SRC") {
        let local_source = local_source.trim();
        if !local_source.is_empty() {
            println!("cargo:rerun-if-changed={local_source}");
            return Ok(PathBuf::from(local_source));
        }
    }

    fetch_game_source(out_dir, game_source)
}

fn fetch_game_source(
    out_dir: &Path,
    game_source: &GameSource,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cache_root = out_dir.join("game-src");
    let marker = cache_root.join(".fetched");
    let marker_value = format!("{}@{}", game_source.repo, game_source.tag);

    if marker.is_file() {
        let cached = fs::read_to_string(&marker)?;
        if cached.trim() == marker_value {
            if let Some(root) = find_game_root(&cache_root) {
                return Ok(root);
            }
        }
    }

    if cache_root.exists() {
        fs::remove_dir_all(&cache_root)?;
    }
    fs::create_dir_all(&cache_root)?;

    let archive_path = out_dir.join("game-source.tar.gz");
    let url = format!(
        "https://github.com/{}/archive/refs/tags/{}.tar.gz",
        game_source.repo, game_source.tag
    );

    let status = Command::new("curl")
        .arg("-fsSL")
        .arg(&url)
        .arg("-o")
        .arg(&archive_path)
        .status()?;
    if !status.success() {
        panic!("failed to download game source from {url}");
    }

    extract_tar_gz(&archive_path, &cache_root)?;
    fs::write(&marker, marker_value)?;

    find_game_root(&cache_root).ok_or_else(|| {
        format!(
            "could not locate extracted game source under {}",
            cache_root.display()
        )
        .into()
    })
}

fn find_game_root(cache_root: &Path) -> Option<PathBuf> {
    let direct = cache_root.join("CMakeLists.txt");
    if direct.is_file() {
        return Some(cache_root.to_path_buf());
    }

    for entry in fs::read_dir(cache_root).ok()? {
        let entry = entry.ok()?;
        if entry.file_type().ok()?.is_dir()
            && entry.path().join("CMakeLists.txt").is_file()
        {
            return Some(entry.path());
        }
    }

    None
}

fn extract_tar_gz(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let archive_file = File::open(archive_path)?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        entry.unpack_in(destination)?;
    }

    Ok(())
}

fn profile_build_type() -> &'static str {
    if std::env::var("PROFILE").as_deref() == Ok("debug") {
        "Debug"
    } else {
        "Release"
    }
}

fn read_bundle_version(game_root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let version_header = fs::read_to_string(game_root.join("src/version.h"))?;
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
