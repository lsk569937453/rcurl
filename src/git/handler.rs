use crate::cli::app_config::Cli;
use crate::git::analyzer;
use crate::git::report;
use crate::response::res::RcurlResponse;
use std::path::PathBuf;
use std::time::Instant;

pub async fn git_statistic_command(
    target: String,
    cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    let target_path = PathBuf::from(&target);
    if !target_path.exists() {
        return Err(anyhow::anyhow!("git: path does not exist: {}", target));
    }

    // Discover the repository root so the command works from subdirectories too.
    let repo = git2::Repository::discover(&target_path).map_err(|_| {
        anyhow::anyhow!("not a git repository (or any of the parent directories): {}", target)
    })?;
    let repo_dir: PathBuf = repo
        .workdir()
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.path().to_path_buf());
    drop(repo);

    let start = Instant::now();

    let stats = analyzer::analyze(&repo_dir)?;

    // Resolve the output path: -o/--output wins, else "./<project>-git-statistics.html".
    let output_path: PathBuf = match cli.file_path_option.as_deref() {
        Some(p) if p != "none" && !p.is_empty() => PathBuf::from(p),
        _ => PathBuf::from(format!(
            "./{}-git-statistics.html",
            stats.git_base_info.project_name
        )),
    };

    let html = report::render(&stats)?;
    std::fs::write(&output_path, html)?;

    let elapsed = start.elapsed();
    let duration_str = format_duration(elapsed);

    println!("Git statistics for: {}", repo_dir.display());
    println!("{}", "-".repeat(52));
    println!("Project:       {}", stats.git_base_info.project_name);
    println!("Commits:       {}", stats.git_base_info.total_commits);
    println!("Authors:       {}", stats.git_base_info.authors);
    println!("Tags:          {}", stats.tag_statistic_info.total_tags);
    println!(
        "Total lines:   {} (+{}/-{})",
        stats.git_base_info.total_lines,
        stats.git_base_info.total_added,
        stats.git_base_info.total_deleted
    );
    println!("{}", "-".repeat(52));
    println!("Report saved to: {}", output_path.display());
    println!("Time elapsed: {}", duration_str);

    Ok(RcurlResponse::GitStat(()))
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
