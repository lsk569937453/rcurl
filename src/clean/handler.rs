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

/// Directories that are protected from cleaning: a nested checkout of rcurl
/// itself keeps its build output, so `rcurl clean <code root>` does not force a
/// full recompile of the tool. The scan root is exempt, so cleaning *inside* the
/// rcurl repository (`rcurl clean .`) still works.
const PROTECTED_DIRS: &[&str] = &["rcurl"];

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

    let (dirs, protected) = find_clean_dirs(path_obj);

    if dirs.is_empty() {
        println!("No build artifacts found under: {}", path_obj.display());
        if !protected.is_empty() {
            println!(
                "Skipped {} protected {}: {}",
                protected.len(),
                plural_directory(protected.len()),
                protected_list(path_obj, &protected)
            );
        }
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

    if !protected.is_empty() {
        println!(
            "Skipped {} protected {}: {}. Delete them explicitly if needed.",
            protected.len(),
            plural_directory(protected.len()),
            protected_list(path_obj, &protected)
        );
    }

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

/// Walk `path` and collect every directory that clean should remove, plus the
/// protected directories that were deliberately left alone.
///
/// No ignore filtering is applied on purpose: target/ and node_modules/ are
/// usually gitignored, yet they are exactly what we are looking for.
fn find_clean_dirs(path: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    // The root itself may be a build directory (e.g. `rcurl clean ./target`).
    if path.is_dir() && is_clean_dir(path) {
        return (vec![path.to_path_buf()], Vec::new());
    }

    let mut dirs = Vec::new();
    let mut protected = Vec::new();
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
        // Only nested checkouts are protected: an explicitly targeted rcurl
        // directory is still cleaned, including `rcurl clean .` inside it.
        if entry.depth() > 0 && is_protected_dir(&name) {
            protected.push(entry.into_path());
            it.skip_current_dir();
            continue;
        }
        if is_clean_name(&name) {
            dirs.push(entry.into_path());
            it.skip_current_dir();
        }
    }

    (dirs, protected)
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

/// Whether a directory is a protected checkout name. Compared case-insensitively
/// so a `RCurl` directory on a case-insensitive filesystem is protected too.
fn is_protected_dir(name: &str) -> bool {
    PROTECTED_DIRS
        .iter()
        .any(|protected| name.eq_ignore_ascii_case(protected))
}

fn plural_directory(count: usize) -> &'static str {
    if count == 1 {
        "directory"
    } else {
        "directories"
    }
}

/// Render protected directories relative to the scan root, comma separated.
fn protected_list(root: &Path, protected: &[PathBuf]) -> String {
    protected
        .iter()
        .map(|dir| display_relative(root, dir))
        .collect::<Vec<_>>()
        .join(", ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create a unique empty temporary directory for a traversal test.
    fn temp_scan_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rcurl-clean-test-{}-{}-{}",
            std::process::id(),
            unique,
            label
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn protected_name_matches_case_insensitively() {
        assert!(is_protected_dir("rcurl"));
        assert!(is_protected_dir("RCurl"));
        assert!(!is_protected_dir("rcurl-ext"));
        assert!(!is_protected_dir("myrcurl"));
    }

    #[test]
    fn find_clean_dirs_skips_nested_rcurl_checkout() {
        let root = temp_scan_root("nested");
        let demo_target = root.join("demo").join("target");
        let rcurl_target = root.join("rcurl").join("target");
        let rcurl_node_modules = root.join("rcurl").join("node_modules");
        for dir in [&demo_target, &rcurl_target, &rcurl_node_modules] {
            std::fs::create_dir_all(dir).unwrap();
        }

        let (dirs, protected) = find_clean_dirs(&root);

        assert_eq!(dirs, vec![demo_target]);
        assert_eq!(protected, vec![root.join("rcurl")]);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_clean_dirs_cleans_inside_an_explicitly_targeted_rcurl_dir() {
        let root = temp_scan_root("explicit");
        let rcurl_root = root.join("rcurl");
        let target = rcurl_root.join("target");
        std::fs::create_dir_all(&target).unwrap();

        let (dirs, protected) = find_clean_dirs(&rcurl_root);

        assert_eq!(dirs, vec![target]);
        assert!(protected.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_clean_dirs_still_ignores_version_control_dirs() {
        let root = temp_scan_root("vcs");
        let git_target = root.join(".git").join("target");
        std::fs::create_dir_all(&git_target).unwrap();

        let (dirs, protected) = find_clean_dirs(&root);

        assert!(dirs.is_empty());
        assert!(protected.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
