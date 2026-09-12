use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use walkdir::WalkDir;

/// Directories removed by `rcurl clean`: dependency caches and build outputs
/// that are fully regenerable (cargo/npm/gradle/... recreate them from source).
const CLEAN_DIRS: &[&str] = &[
    // Rust / JVM (cargo, maven)
    "target",
    // Node.js / frontend
    "node_modules",
    ".next",
    ".nuxt",
    ".turbo",
    ".parcel-cache",
    ".sass-cache",
    "dist",
    // Generic build outputs (C/C++/Java/.NET/...)
    "build",
    "out",
    "obj",
    ".gradle",
    "Pods",
    // Python
    "venv",
    ".venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
];

/// Directory name prefixes that also mark build outputs (e.g. `target-debug`
/// from cargo workspaces, `cmake-build-debug` from CLion).
const CLEAN_DIR_PREFIXES: &[&str] = &["target-", "cmake-build-"];

/// Version control internals: never cleaned and never descended into.
const NEVER_TOUCH_DIRS: &[&str] = &[".git", ".hg", ".svn"];

pub async fn clean_command(
    target: String,
    yes: bool,
    _cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    let path_obj = Path::new(&target);

    if !path_obj.exists() {
        return Err(anyhow!("clean: path does not exist: {}", target));
    }

    let start = Instant::now();

    let dirs = find_clean_dirs(path_obj);

    if dirs.is_empty() {
        println!("No build artifacts found under: {}", path_obj.display());
        return Ok(RcurlResponse::Clean(()));
    }

    // Compute directory sizes in parallel; node_modules can be huge.
    let total = dirs.len();
    let pb = ProgressBar::new(total as u64);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{pos}/{len}] {msg}")?);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let done = AtomicUsize::new(0);
    let mut sized: Vec<(PathBuf, u64)> = dirs
        .par_iter()
        .map(|dir| {
            let name = dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let size = calculate_dir_size_parallel(dir);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            pb.set_message(format!("Scanned [{}/{}]: {}", n, total, name));
            pb.inc(1);
            (dir.clone(), size)
        })
        .collect();

    pb.finish_with_message("Scan complete!");

    sized.sort_by(|a, b| b.1.cmp(&a.1));
    let total_size: u64 = sized.iter().map(|(_, s)| s).sum();

    // Show what will be removed before touching anything.
    println!("Clean build artifacts for: {}", path_obj.display());
    println!("{}", "-".repeat(76));
    for (dir, size) in &sized {
        println!(
            "{:>12}   {}",
            format_bytes(*size),
            display_relative(path_obj, dir)
        );
    }
    println!("{}", "-".repeat(76));
    println!(
        "Found {} directories, {} total.",
        sized.len(),
        format_bytes(total_size)
    );

    if !yes && !confirm("Delete them?")? {
        println!("Cancelled.");
        return Ok(RcurlResponse::Clean(()));
    }

    // Delete sequentially; a locked file inside one directory must not
    // abort the removal of the others.
    let pb = ProgressBar::new(total as u64);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{pos}/{len}] {msg}")?);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut deleted = 0usize;
    let mut freed = 0u64;
    let mut failed: Vec<(PathBuf, std::io::Error)> = Vec::new();
    for (i, (dir, size)) in sized.iter().enumerate() {
        pb.set_message(format!("Deleting [{}/{}]: {}", i + 1, total, dir.display()));
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {
                deleted += 1;
                freed += size;
            }
            Err(e) => failed.push((dir.clone(), e)),
        }
        pb.inc(1);
    }
    pb.finish_with_message("Clean complete!");

    if !failed.is_empty() {
        println!();
        println!("Failed to delete {} directories:", failed.len());
        for (dir, e) in &failed {
            println!("  {} ({})", dir.display(), e);
        }
    }

    let duration_str = format_duration(start.elapsed());
    println!();
    println!(
        "Deleted {}/{} directories, freed {}.",
        deleted,
        total,
        format_bytes(freed)
    );
    println!("Time elapsed: {}", duration_str);

    Ok(RcurlResponse::Clean(()))
}

/// Walk `path` and collect every directory that clean should remove.
/// No ignore filtering is applied on purpose: target/ and node_modules/
/// are usually gitignored, yet they are exactly what we are looking for.
fn find_clean_dirs(path: &Path) -> Vec<PathBuf> {
    // The root itself may be a build directory (e.g. `rcurl clean ./target`).
    if path.is_dir() && is_clean_dir(path) {
        return vec![path.to_path_buf()];
    }

    let mut dirs = Vec::new();
    let mut it = WalkDir::new(path).follow_links(false).into_iter();

    while let Some(entry) = it.next() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if NEVER_TOUCH_DIRS.contains(&name.as_ref()) {
            it.skip_current_dir();
            continue;
        }
        if is_clean_name(&name) {
            dirs.push(entry.into_path());
            it.skip_current_dir();
        }
    }

    dirs
}

/// Whether a path's final component marks it as a build artifact directory.
fn is_clean_dir(path: &Path) -> bool {
    path.file_name()
        .map(|n| is_clean_name(&n.to_string_lossy()))
        .unwrap_or(false)
}

fn is_clean_name(name: &str) -> bool {
    CLEAN_DIRS.contains(&name) || CLEAN_DIR_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Show the path relative to the scan root when possible; falls back to the
/// full path when the root is itself the matched directory.
fn display_relative(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root)
        .ok()
        .and_then(|p| {
            let s = p.display().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .unwrap_or_else(|| dir.display().to_string())
}

/// Ask the user to confirm deletion. Anything but an affirmative answer
/// (including EOF on a non-interactive stdin) cancels the clean.
fn confirm(prompt: &str) -> Result<bool, anyhow::Error> {
    print!("{} [y/N] ", prompt);
    std::io::stdout().flush()?;

    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Parallel directory size calculation (same approach as the disk command).
fn calculate_dir_size_parallel(dir_path: &Path) -> u64 {
    WalkDir::new(dir_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn format_bytes(bytes: u64) -> String {
    const TB: u64 = 1024 * 1024 * 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration in human-readable format
fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f64();

    if secs >= 60.0 {
        let minutes = (secs / 60.0).floor();
        let seconds = secs % 60.0;
        format!("{}m {:.2}s", minutes, seconds)
    } else if secs >= 1.0 {
        format!("{:.2}s", secs)
    } else {
        let millis = duration.as_millis();
        format!("{}ms", millis)
    }
}
