use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use anyhow::Context;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

pub async fn disk_size_command(path: String, _cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let path_obj = Path::new(&path);

    // Check if path exists
    if !path_obj.exists() {
        return Err(anyhow::anyhow!("disk-size: path does not exist: {}", path));
    }

    // Start timer
    let start = Instant::now();

    // Get disk usage
    let entries = get_disk_usage(path_obj)?;

    // Calculate elapsed time
    let elapsed = start.elapsed();
    let duration_str = format_duration(elapsed);

    // Display results
    println!("Disk usage for: {}", path_obj.display());
    println!("{:<50} {:>12} {:>12}", "Name", "Size", "Type");
    println!("{}", "-".repeat(76));

    for entry in entries {
        let name_width = display_width(&entry.display_name);
        let padding = 50usize.saturating_sub(name_width);
        let padding_str = " ".repeat(padding);
        println!(
            "{}{} {:>12} {:>12}",
            entry.display_name, padding_str, entry.size, entry.file_type
        );
    }

    println!();
    println!("Time elapsed: {}", duration_str);

    Ok(RcurlResponse::DiskSize(()))
}

struct DiskEntry {
    name: String,
    display_name: String,
    size: String,
    file_type: String,
}

/// Calculate the display width of a string (CJK characters count as 2)
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            if c.is_ascii() {
                1
            } else {
                // CJK and other wide characters typically take 2 columns
                2
            }
        })
        .sum()
}

/// Truncate string to fit within max display width
fn truncate_for_display(s: &str, max_width: usize) -> String {
    let mut current_width = 0;
    let mut result = String::new();

    for c in s.chars() {
        let char_width = if c.is_ascii() { 1 } else { 2 };
        if current_width + char_width > max_width {
            break;
        }
        result.push(c);
        current_width += char_width;
    }

    if result.len() < s.len() {
        // Add ellipsis if truncated
        let ellipsis = "...";
        while display_width(&result) + display_width(ellipsis) > max_width && !result.is_empty() {
            result.pop();
            // If last char was wide, we need to account for that
            let last_char_width = result
                .chars()
                .last()
                .map(|c| if c.is_ascii() { 1 } else { 2 })
                .unwrap_or(1);
            current_width = current_width.saturating_sub(last_char_width);
        }
        result.push_str(ellipsis);
    }

    result
}

fn get_disk_usage(path: &Path) -> Result<Vec<DiskEntry>, anyhow::Error> {
    let mut entries = Vec::new();

    if path.is_file() {
        let metadata = path.metadata()?;
        let size = format_bytes(metadata.len());
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let display_name = truncate_for_display(&name, 48);
        entries.push(DiskEntry {
            name,
            display_name,
            size,
            file_type: "FILE".to_string(),
        });
    } else {
        // Collect top-level entries
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in WalkDir::new(path)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path() != path)
        {
            let ft = entry.file_type();
            let path_buf: PathBuf = entry.path().to_path_buf();

            if ft.is_dir() {
                dirs.push(path_buf);
            } else if ft.is_file()
                && let Ok(metadata) = entry.metadata()
            {
                files.push((path_buf, metadata.len()));
            }
        }

        // Create spinning progress bar
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_bar().template("{spinner:.green} {msg}")?);
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        // Calculate directory sizes in parallel with progress indicator
        let mut dir_sizes: Vec<(PathBuf, u64)> = dirs
            .par_iter() // Parallel iteration
            .enumerate()
            .map(|(idx, dir_path)| {
                let dir_name = dir_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                pb.set_message(format!(
                    "Scanning [{}/{}]: {}",
                    idx + 1,
                    dirs.len(),
                    dir_name
                ));

                let size = calculate_dir_size_parallel(dir_path);
                (dir_path.clone(), size)
            })
            .collect();

        pb.finish_with_message("Scan complete!");

        // Sort by size (descending)
        dir_sizes.sort_by(|a, b| b.1.cmp(&a.1));
        files.sort_by(|a, b| b.1.cmp(&a.1));

        // Add directories first
        for (dir_path, size) in dir_sizes {
            let name = dir_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let display_name = truncate_for_display(&name, 48);
            entries.push(DiskEntry {
                name,
                display_name,
                size: format_bytes(size),
                file_type: "DIR".to_string(),
            });
        }

        // Then add files
        for (file_path, size) in files {
            let name = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let display_name = truncate_for_display(&name, 48);
            entries.push(DiskEntry {
                name,
                display_name,
                size: format_bytes(size),
                file_type: "FILE".to_string(),
            });
        }
    }

    Ok(entries)
}

/// Parallel version of calculate_dir_size for rayon
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
