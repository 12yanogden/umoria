use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use flate2::read::GzDecoder;
use tar::Archive;

const BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bundle.tar.gz"));
const BUNDLE_VERSION: &str = env!("BUNDLE_VERSION");
const SAVE_FILE_NAME: &str = "game.sav";
const SCORES_FILE_NAME: &str = "scores.dat";
const SCORES_URL: &str =
    "https://raw.githubusercontent.com/dungeons-of-moria/umoria/master/data/scores.dat";

fn main() -> Result<()> {
    let data_dir = resolve_data_dir()?;
    ensure_bundle(&data_dir)?;
    splash(&data_dir)
}

fn resolve_data_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("UMORIA_DATA_DIR") {
        if path.trim().is_empty() {
            bail!("UMORIA_DATA_DIR must not be empty");
        }
        return Ok(PathBuf::from(path));
    }

    let project_dirs = ProjectDirs::from("", "", "umoria")
        .context("resolve a per-user data directory for umoria")?;
    Ok(project_dirs.data_dir().to_path_buf())
}

fn ensure_bundle(data_dir: &Path) -> Result<()> {
    fs::create_dir_all(data_dir).with_context(|| format!("create {}", data_dir.display()))?;

    let marker = data_dir.join(".bundle_version");
    let installed_version = fs::read_to_string(&marker).unwrap_or_default();
    let needs_upgrade = installed_version.trim() != BUNDLE_VERSION;
    let missing_binary = !data_dir.join("umoria").is_file();

    if needs_upgrade || missing_binary {
        extract_bundle(data_dir, true)?;
        fs::write(&marker, BUNDLE_VERSION)
            .with_context(|| format!("write {}", marker.display()))?;
    }

    if !data_dir.join(SCORES_FILE_NAME).is_file() {
        extract_bundle(data_dir, false)?;
    }

    set_executable(data_dir.join("umoria"))?;
    Ok(())
}

fn extract_bundle(data_dir: &Path, overwrite_game_files: bool) -> Result<()> {
    let decoder = GzDecoder::new(BUNDLE);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let relative = path
            .to_str()
            .with_context(|| format!("bundle path is not valid UTF-8: {}", path.display()))?;

        if relative == SCORES_FILE_NAME && data_dir.join(SCORES_FILE_NAME).exists() {
            continue;
        }

        if !overwrite_game_files && relative != SCORES_FILE_NAME {
            continue;
        }

        let destination = data_dir.join(relative);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&destination)?;
            continue;
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut contents = Vec::new();
        entry.read_to_end(&mut contents)?;
        fs::write(&destination, contents)
            .with_context(|| format!("write {}", destination.display()))?;
    }

    Ok(())
}

#[cfg(unix)]
fn set_executable(path: PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if path.is_file() {
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: PathBuf) -> Result<()> {
    Ok(())
}

fn splash(data_dir: &Path) -> Result<()> {
    loop {
        println!("Welcome to Umoria!");
        divider();
        match menu(&["Resume", "Load", "Reset", "Exit"])? {
            0 => launch(data_dir)?,
            1 => load(data_dir)?,
            2 => {
                match menu(&["Yes", "No"])? {
                    0 => {
                        reset_save_file(data_dir)?;
                        reset_scores(data_dir)?;
                    }
                    _ => println!("Reset cancelled."),
                }
                skip_line();
            }
            3 => {
                println!("Goodbye.");
                break;
            }
            _ => unreachable!("menu only returns valid indices"),
        }
    }

    Ok(())
}

fn launch(data_dir: &Path) -> Result<()> {
    let game_binary = data_dir.join("umoria");
    let status = Command::new(&game_binary)
        .current_dir(data_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("run {}", game_binary.display()))?;

    if !status.success() {
        bail!("umoria exited with {status}");
    }

    archive(data_dir)?;
    Ok(())
}

fn archive(data_dir: &Path) -> Result<()> {
    let save_file = data_dir.join(SAVE_FILE_NAME);
    if !save_file.is_file() {
        return Ok(());
    }

    let archive_dir = data_dir.join("_archive");
    fs::create_dir_all(&archive_dir)?;

    let archive_name = chrono_like_timestamp();
    let archive_path = archive_dir.join(archive_name);
    fs::copy(&save_file, &archive_path).with_context(|| {
        format!(
            "archive {} to {}",
            save_file.display(),
            archive_path.display()
        )
    })?;
    Ok(())
}

fn load(data_dir: &Path) -> Result<()> {
    let archive_dir = data_dir.join("_archive");
    let archives = list_archives(&archive_dir)?;
    if archives.is_empty() {
        println!("No saved games found. Please archive a save first.");
        return Ok(());
    }

    divider();
    skip_line();
    let mut menu_options: Vec<String> = archives.iter().map(|name| name.to_string()).collect();
    menu_options.push("Back".to_string());
    let menu_refs: Vec<&str> = menu_options.iter().map(String::as_str).collect();
    let selected_index = menu(&menu_refs)?;
    if selected_index == archives.len() {
        return Ok(());
    }
    let selected_archive = archives[selected_index].clone();

    let marked_archive = mark_last_loaded(&archive_dir, &selected_archive)?;
    let save_file = data_dir.join(SAVE_FILE_NAME);
    if save_file.exists() {
        fs::remove_file(&save_file)?;
    }

    fs::copy(
        archive_dir.join(&marked_archive),
        &save_file,
    )
    .with_context(|| format!("restore archive {}", marked_archive))?;

    launch(data_dir)
}

fn reset_save_file(data_dir: &Path) -> Result<()> {
    let save_file = data_dir.join(SAVE_FILE_NAME);
    if save_file.exists() {
        fs::remove_file(&save_file)
            .with_context(|| format!("remove {}", save_file.display()))?;
        println!("Successfully removed {SAVE_FILE_NAME}");
    }
    Ok(())
}

fn reset_scores(data_dir: &Path) -> Result<()> {
    let scores_targets = [
        data_dir.join("data").join(SCORES_FILE_NAME),
        data_dir.join(SCORES_FILE_NAME),
    ];

    for scores_file in scores_targets {
        if scores_file.exists() {
            fs::remove_file(&scores_file)
                .with_context(|| format!("remove {}", scores_file.display()))?;
        }

        download_scores(&scores_file)?;
        println!(
            "Successfully updated {}",
            scores_file.file_name().unwrap().to_string_lossy()
        );
    }

    Ok(())
}

fn download_scores(destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let status = Command::new("curl")
        .arg("-fsSL")
        .arg(SCORES_URL)
        .arg("-o")
        .arg(destination)
        .status()
        .context("run curl to download scores.dat")?;

    if status.success() {
        return Ok(());
    }

    bail!("failed to download {SCORES_URL}");
}

fn mark_last_loaded(archive_dir: &Path, archive_to_mark: &str) -> Result<String> {
    for entry in fs::read_dir(archive_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.ends_with("_last_loaded") {
            let unmarked = file_name.trim_end_matches("_last_loaded").to_string();
            fs::rename(entry.path(), archive_dir.join(&unmarked))?;
        }
    }

    let marked_name = format!("{archive_to_mark}_last_loaded");
    fs::rename(
        archive_dir.join(archive_to_mark),
        archive_dir.join(&marked_name),
    )?;
    println!("{marked_name}");
    Ok(marked_name)
}

fn list_archives(archive_dir: &Path) -> Result<Vec<String>> {
    if !archive_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(archive_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        entries.push((entry.path(), entry.metadata()?.modified()?));
    }

    entries.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    Ok(entries
        .into_iter()
        .map(|(path, _)| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect())
}

fn menu(options: &[&str]) -> Result<usize> {
    loop {
        skip_line();
        for (index, option) in options.iter().enumerate() {
            println!("{index}: {option}");
        }
        skip_line();
        print!("Select an option: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let selection = input.trim();

        let Ok(index) = selection.parse::<usize>() else {
            println!("Invalid selection: {selection}. Please select a number between 0 and {}.", options.len() - 1);
            continue;
        };

        if index >= options.len() {
            println!(
                "Invalid selection: {index}. Please select a number between 0 and {}.",
                options.len() - 1
            );
            continue;
        }

        skip_line();
        return Ok(index);
    }
}

fn divider() {
    println!("------------------");
}

fn skip_line() {
    println!();
}

fn chrono_like_timestamp() -> String {
    let output = Command::new("date")
        .arg("+%Y.%m.%d_%H.%M.%S")
        .output()
        .context("run date for archive timestamp");

    match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string(),
        _ => SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}
