use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use ignore::WalkBuilder;

/// Directories to skip during traversal (version control, IDE, dependency and
/// build caches that never contain meaningful source code).
const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    ".idea",
    ".vscode",
    ".venv",
    "venv",
    "__pycache__",
];

pub async fn count_lines_command(path: String, _cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let path_obj = Path::new(&path);

    // Check if path exists
    if !path_obj.exists() {
        return Err(anyhow::anyhow!("count: path does not exist: {}", path));
    }

    // Start timer
    let start = Instant::now();

    // Collect all candidate files
    let files = collect_files(path_obj)?;

    // Count lines in parallel
    let total_files = files.len();
    let pb = indicatif::ProgressBar::new(total_files as u64);
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.green} [{pos}/{len}] {msg}")?,
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let done = AtomicUsize::new(0);
    let results: Vec<(PathBuf, u64)> = files
        .par_iter()
        .filter_map(|file| {
            let lines = count_lines(file).ok()?;
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            pb.set_message(format!("Counted [{}/{}]: {}", n, total_files, file.display()));
            pb.inc(1);
            Some((file.clone(), lines))
        })
        .collect();

    pb.finish_with_message("Count complete!");

    // Aggregate by extension
    let mut ext_map: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for (file, lines) in results {
        let ext = extension_of(&file).to_string();
        let entry = ext_map.entry(ext).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += lines;
    }

    // Calculate elapsed time
    let elapsed = start.elapsed();
    let duration_str = format_duration(elapsed);

    // Display results
    println!("Line count for: {}", path_obj.display());
    println!("{:<20} {:>12} {:>12}", "Extension", "Files", "Lines");
    println!("{}", "-".repeat(46));

    let mut total_files_count = 0u64;
    let mut total_lines = 0u64;
    for (ext, (files, lines)) in &ext_map {
        println!("{:<20} {:>12} {:>12}", ext, files, lines);
        total_files_count += files;
        total_lines += lines;
    }

    println!("{}", "-".repeat(46));
    println!("{:<20} {:>12} {:>12}", "TOTAL", total_files_count, total_lines);
    println!();
    println!("Time elapsed: {}", duration_str);

    Ok(RcurlResponse::Count(()))
}

/// Collect all files under `path` that should be counted.
fn collect_files(path: &Path) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut files = Vec::new();

    if path.is_file() {
        if is_countable(path) {
            files.push(path.to_path_buf());
        }
        return Ok(files);
    }

    for entry in WalkBuilder::new(path)
        .follow_links(false)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .filter_entry(|e| !should_skip_dir(e))
        .build()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) && is_countable(entry.path()) {
            files.push(entry.path().to_path_buf());
        }
    }

    Ok(files)
}

/// Whether a directory entry should always be skipped, even without a
/// .gitignore (e.g. target, node_modules). Entries matched by .gitignore are
/// already excluded by the walker itself.
fn should_skip_dir(entry: &ignore::DirEntry) -> bool {
    if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    SKIP_DIRS.contains(&name.as_ref()) || name.starts_with("target-")
}

/// Whether a file should be counted: must have an extension and not be binary.
fn is_countable(path: &Path) -> bool {
    // Skip files without an extension (no marker in the suffix).
    if extension_of(path).is_empty() {
        return false;
    }
    // Skip binary files.
    !is_binary(path)
}

/// Return the lowercase extension of a path (without the dot), or empty string.
fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Heuristic binary detection: a file is considered binary if it contains a
/// NUL byte in the first chunk of its content.
fn is_binary(path: &Path) -> bool {
    use std::io::Read;

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return true,
    };

    let mut buf = [0u8; 8192];
    let mut read = 0usize;
    loop {
        match file.read(&mut buf[read..]) {
            Ok(0) => break,
            Ok(n) => {
                read += n;
                if read >= buf.len() {
                    break;
                }
            }
            Err(_) => return true,
        }
    }

    buf[..read].contains(&0)
}

/// Count the number of lines in a text file.
fn count_lines(path: &Path) -> std::io::Result<u64> {
    use std::io::BufRead;

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut lines = 0u64;
    for line in reader.lines() {
        let _ = line?;
        lines += 1;
    }
    Ok(lines)
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
