use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use anyhow::anyhow;
use git2::Repository;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

/// Directories that are never crawled while looking for repositories: version
/// control internals plus dependency and build caches that cannot hold a
/// checkout worth pulling. Repositories ignored by their own `.gitignore` are
/// already excluded by the walker.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".venv",
    "venv",
    "__pycache__",
    ".gradle",
    ".next",
];

/// One repository whose remote URL contains the search keyword, together with
/// the information needed to decide how `git pull` should be invoked.
struct RemoteMatch {
    path: PathBuf,
    remote: String,
    url: String,
    branch: Option<String>,
    upstream_matches: bool,
}

/// One repository scheduled for a concurrent `git pull`.
struct PullJob {
    index: usize,
    path: PathBuf,
    remote: String,
    url: String,
    branch: Option<String>,
    args: Vec<String>,
}

/// Outcome of one concurrent pull.
enum PullResult {
    Pulled { output: String },
    Failed { reason: String, output: String },
    SpawnFailed { error: String },
    Cancelled,
}

/// A completed concurrent pull, including the job it ran.
struct PullAttempt {
    job: PullJob,
    result: PullResult,
}

/// Why a pull failed, derived from git's own output. The category only drives
/// the summary wording; the repository is always reported as failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureKind {
    Authentication,
    LocalChanges,
    Conflict,
    Diverged,
    RemoteMissing,
    Network,
    Other,
}

/// One failed pull, kept for the final summary together with the command the
/// user can run by hand.
struct PullFailure {
    path: PathBuf,
    remote: String,
    url: String,
    branch: Option<String>,
    /// The observed exit status or spawn error.
    status: String,
    kind: FailureKind,
    /// Captured git output; `None` for serial pulls, which stream to the terminal.
    output: Option<String>,
    /// Copy-paste command that retries this repository's pull manually.
    manual: String,
}

/// One repository that matched the keyword but was not pulled.
struct SkippedRepo {
    path: PathBuf,
    reason: String,
}

/// Aggregated result of one `rcurl pull` run.
#[derive(Default)]
struct PullSummary {
    scanned: usize,
    matched: usize,
    /// Number of pulls listed by `--dry-run`.
    planned: usize,
    pulled: usize,
    failures: Vec<PullFailure>,
    skipped: Vec<SkippedRepo>,
    /// Jobs that never ran because git could not be started.
    aborted: usize,
}

/// How many trailing output lines are echoed per failure in the summary; the
/// complete output block is already printed above.
const FAILURE_OUTPUT_LINES: usize = 12;

/// Output needles that classify a failure, checked in order so the most
/// specific cause wins over a generic network message.
const FAILURE_PATTERNS: &[(&[&str], FailureKind)] = &[
    (
        &[
            "could not read username",
            "could not read password",
            "authentication failed",
            "invalid username or password",
            "permission denied (publickey",
            "terminal prompts disabled",
            "unable to get password",
            "access denied",
            "returned error: 401",
            "returned error: 403",
        ],
        FailureKind::Authentication,
    ),
    (
        &[
            "your local changes",
            "would be overwritten",
            "unstaged changes",
            "uncommitted changes",
            "commit your changes or stash",
        ],
        FailureKind::LocalChanges,
    ),
    (
        &[
            "merge conflict",
            "automatic merge failed",
            "fix conflicts",
            "concluded your merge",
            "rebase in progress",
        ],
        FailureKind::Conflict,
    ),
    (
        &[
            "divergent branches",
            "how to reconcile",
            "not possible to fast-forward",
            "non-fast-forward",
        ],
        FailureKind::Diverged,
    ),
    (
        &[
            "couldn't find remote ref",
            "does not appear to be a git repository",
            "repository not found",
            "no upstream branch",
            "no such remote",
            "no such ref was fetched",
        ],
        FailureKind::RemoteMissing,
    ),
    (
        &[
            "could not resolve host",
            "failed to connect",
            "connection timed out",
            "connection refused",
            "network is unreachable",
            "rpc failed",
            "remote end hung up",
            "ssl certificate",
            "tls",
            "connection reset",
            "timed out",
            "early eof",
        ],
        FailureKind::Network,
    ),
];

impl FailureKind {
    /// Classify one captured `git pull` output. Anything unrecognised stays
    /// `Other`, which still reports the repository and its manual command.
    fn from_output(output: &str) -> Self {
        let text = output.to_lowercase();
        for (needles, kind) in FAILURE_PATTERNS {
            if needles.iter().any(|needle| text.contains(needle)) {
                return *kind;
            }
        }
        Self::Other
    }

    /// Short cause shown in the failure summary.
    fn label(self) -> &'static str {
        match self {
            Self::Authentication => "authentication required",
            Self::LocalChanges => "local changes block the pull",
            Self::Conflict => "unresolved merge or rebase",
            Self::Diverged => "local and remote branches diverged",
            Self::RemoteMissing => "remote repository or branch missing",
            Self::Network => "network or TLS error",
            Self::Other => "git pull failed",
        }
    }

    /// What to do about it before retrying, or before pulling by hand.
    fn hint(self) -> &'static str {
        match self {
            Self::Authentication => {
                "run with -j 1 for interactive prompts, or configure an SSH key / credential helper"
            }
            Self::LocalChanges => "commit or stash the local changes in that repository first",
            Self::Conflict => "resolve the conflict and finish the merge/rebase first",
            Self::Diverged => {
                "inspect the divergence, then pull manually with --rebase or --no-rebase"
            }
            Self::RemoteMissing => {
                "check the remote and branch with git remote -v / git branch -vv"
            }
            Self::Network => "check proxy, VPN and DNS, then retry",
            Self::Other => "fix the cause shown in the git output above, then retry",
        }
    }
}

/// Pull every git repository under `target` that has a remote URL containing
/// `keyword`.
pub async fn pull_command(
    keyword: String,
    target: String,
    dry_run: bool,
    jobs: u16,
    _cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return Err(anyhow!("pull: keyword must not be empty"));
    }

    let root = Path::new(&target);
    if !root.exists() {
        return Err(anyhow!("pull: path does not exist: {}", target));
    }
    if !root.is_dir() {
        return Err(anyhow!("pull: path is not a directory: {}", target));
    }

    let start = Instant::now();
    let keyword_lower = keyword.to_lowercase();

    let repos = find_git_repos(root);
    let scanned = repos.len();

    let mut matched: Vec<RemoteMatch> = Vec::new();
    let mut skipped: Vec<SkippedRepo> = Vec::new();

    for repo_path in &repos {
        match open_and_match(repo_path, &keyword_lower) {
            Ok(Some(found)) => matched.push(found),
            Ok(None) => {}
            Err(err) => skipped.push(SkippedRepo {
                path: repo_path.clone(),
                reason: format!("cannot open repository: {err}"),
            }),
        }
    }

    let mut summary = PullSummary {
        scanned,
        matched: matched.len(),
        ..PullSummary::default()
    };

    if matched.is_empty() {
        println!(
            "No git repositories with a remote matching '{}' under: {}",
            keyword,
            root.display()
        );
        summary.skipped = skipped;
        print_pull_summary(&summary, root, keyword, dry_run, start.elapsed());
        return Ok(RcurlResponse::Pull(()));
    }

    let total = matched.len();
    let runnable = matched
        .iter()
        .filter(|found| {
            pull_args(
                found.upstream_matches,
                &found.remote,
                found.branch.as_deref(),
            )
            .is_some()
        })
        .count();
    let workers = effective_workers(jobs, runnable);

    let mut fatal: Option<anyhow::Error> = None;

    if dry_run || workers == 1 {
        for (index, found) in matched.iter().enumerate() {
            println!("[{}/{}] {}", index + 1, total, found.path.display());
            println!("      {} -> {}", found.remote, found.url);
            std::io::stdout().flush()?;

            let args = pull_args(
                found.upstream_matches,
                &found.remote,
                found.branch.as_deref(),
            );
            let Some(args) = args else {
                println!("      skipped: not on a branch");
                skipped.push(SkippedRepo {
                    path: found.path.clone(),
                    reason: "not on a branch (detached HEAD)".to_string(),
                });
                continue;
            };

            if dry_run {
                println!("      {}", manual_pull_command(&found.path, &args));
                summary.planned += 1;
                continue;
            }

            match run_git_pull(&found.path, &args) {
                Ok(status) if status.success() => summary.pulled += 1,
                Ok(status) => summary.failures.push(PullFailure {
                    path: found.path.clone(),
                    remote: found.remote.clone(),
                    url: found.url.clone(),
                    branch: found.branch.clone(),
                    status: format!("git pull exited with {status}"),
                    kind: FailureKind::Other,
                    // Serial pulls stream git's output, so only the exit status
                    // is available for the summary.
                    output: None,
                    manual: manual_pull_command(&found.path, &args),
                }),
                Err(err) => {
                    summary.failures.push(PullFailure {
                        path: found.path.clone(),
                        remote: found.remote.clone(),
                        url: found.url.clone(),
                        branch: found.branch.clone(),
                        status: format!("failed to run git: {err}"),
                        kind: FailureKind::Other,
                        output: None,
                        manual: manual_pull_command(&found.path, &args),
                    });
                    fatal = Some(anyhow!(
                        "pull: failed to run git ({}). Is git installed and on PATH?",
                        err
                    ));
                    break;
                }
            }
        }
    } else {
        let mut pending: Vec<PullJob> = Vec::with_capacity(runnable);
        for (index, found) in matched.iter().enumerate() {
            match pull_args(
                found.upstream_matches,
                &found.remote,
                found.branch.as_deref(),
            ) {
                Some(args) => pending.push(PullJob {
                    index,
                    path: found.path.clone(),
                    remote: found.remote.clone(),
                    url: found.url.clone(),
                    branch: found.branch.clone(),
                    args,
                }),
                None => skipped.push(SkippedRepo {
                    path: found.path.clone(),
                    reason: "not on a branch (detached HEAD)".to_string(),
                }),
            }
        }

        println!(
            "Pulling {} repositories with {} workers (non-interactive; use -j 1 for interactive prompts)...",
            pending.len(),
            workers
        );
        std::io::stdout().flush()?;

        let attempts = run_jobs_parallel(pending, workers)?;
        for attempt in &attempts {
            match &attempt.result {
                PullResult::Pulled { .. } => summary.pulled += 1,
                PullResult::Failed { reason, output } => summary.failures.push(PullFailure {
                    path: attempt.job.path.clone(),
                    remote: attempt.job.remote.clone(),
                    url: attempt.job.url.clone(),
                    branch: attempt.job.branch.clone(),
                    status: reason.clone(),
                    kind: FailureKind::from_output(output),
                    output: Some(output.clone()).filter(|output| !output.trim().is_empty()),
                    manual: manual_pull_command(&attempt.job.path, &attempt.job.args),
                }),
                PullResult::SpawnFailed { error } => {
                    if fatal.is_none() {
                        fatal = Some(anyhow!(
                            "pull: failed to run git ({}). Is git installed and on PATH?",
                            error
                        ));
                    }
                }
                PullResult::Cancelled => summary.aborted += 1,
            }
        }
    }

    summary.skipped = skipped;
    print_pull_summary(&summary, root, keyword, dry_run, start.elapsed());

    if let Some(err) = fatal {
        return Err(err);
    }

    Ok(RcurlResponse::Pull(()))
}

/// Number of workers to use for `repositories` pull jobs. A single repository
/// falls back to the serial path, which keeps interactive git prompts working.
fn effective_workers(requested: u16, repositories: usize) -> usize {
    if repositories == 0 {
        return 1;
    }
    (requested as usize).min(repositories).max(1)
}

/// Run each job on a bounded rayon thread pool, printing each repository's
/// captured output as one block when it finishes. A failure to start git stops
/// further jobs from being launched, but work already in flight still finishes.
fn run_jobs_parallel(
    jobs: Vec<PullJob>,
    workers: usize,
) -> Result<Vec<PullAttempt>, anyhow::Error> {
    let total = jobs.len();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|err| anyhow!("pull: failed to create worker pool: {err}"))?;

    let print_lock = Mutex::new(());
    let done = AtomicUsize::new(0);
    let spawn_failed = AtomicBool::new(false);

    let mut attempts: Vec<PullAttempt> = pool.install(move || {
        jobs.into_par_iter()
            .map(|job| {
                if spawn_failed.load(Ordering::Relaxed) {
                    return PullAttempt {
                        job,
                        result: PullResult::Cancelled,
                    };
                }

                let result = match run_git_pull_captured(&job.path, &job.args) {
                    Ok(output) if output.status.success() => PullResult::Pulled {
                        output: captured_output_text(&output.stdout, &output.stderr),
                    },
                    Ok(output) => PullResult::Failed {
                        reason: format!("git pull exited with {}", output.status),
                        output: captured_output_text(&output.stdout, &output.stderr),
                    },
                    Err(err) => {
                        spawn_failed.store(true, Ordering::Relaxed);
                        PullResult::SpawnFailed {
                            error: err.to_string(),
                        }
                    }
                };

                let attempt = PullAttempt { job, result };
                {
                    let _guard = print_lock
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if !matches!(&attempt.result, PullResult::Cancelled) {
                        let ordinal = done.fetch_add(1, Ordering::Relaxed) + 1;
                        print_parallel_attempt(&attempt, ordinal, total);
                    }
                }
                attempt
            })
            .collect()
    });

    attempts.sort_by_key(|attempt| attempt.job.index);
    Ok(attempts)
}

/// Print one completed repository's header, captured git output and status.
fn print_parallel_attempt(attempt: &PullAttempt, ordinal: usize, total: usize) {
    println!("[{}/{}] {}", ordinal, total, attempt.job.path.display());
    println!("      {} -> {}", attempt.job.remote, attempt.job.url);

    match &attempt.result {
        PullResult::Pulled { output } => {
            print_captured_output(output);
            println!("      done");
        }
        PullResult::Failed { reason, output } => {
            print_captured_output(output);
            println!("      failed: {reason}");
        }
        PullResult::SpawnFailed { error } => {
            println!("      failed to run git: {error}");
        }
        PullResult::Cancelled => {}
    }

    let _ = std::io::stdout().flush();
}

/// Print captured git output indented under the repository header, splitting
/// progress-style carriage returns as well as normal line feeds.
fn print_captured_output(output: &str) {
    for line in output.split(['\r', '\n']) {
        if !line.trim().is_empty() {
            println!("      {line}");
        }
    }
}

/// Combine captured stdout and stderr into one printable block. Invalid UTF-8
/// is replaced rather than failing the pull summary.
fn captured_output_text(stdout: &[u8], stderr: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(stdout).into_owned();
    let stderr = String::from_utf8_lossy(stderr);
    if !stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&stderr);
    }
    text
}

/// Print the run summary: counts, then each failure with its cause, hint and a
/// copy-paste manual pull command, then skipped repositories.
fn print_pull_summary(
    summary: &PullSummary,
    root: &Path,
    keyword: &str,
    dry_run: bool,
    elapsed: std::time::Duration,
) {
    println!();
    println!("{}", "-".repeat(76));
    println!("Summary");
    println!("{}", "-".repeat(76));
    println!(
        "Scanned   : {} git repositories under {}",
        summary.scanned,
        root.display()
    );
    println!(
        "Matched   : {} (remote URL contains \"{}\")",
        summary.matched, keyword
    );
    if dry_run {
        println!(
            "Planned   : {} git pull commands listed above; nothing was modified",
            summary.planned
        );
    } else {
        println!("Pulled    : {}", summary.pulled);
        println!("Failed    : {}", summary.failures.len());
    }
    println!("Skipped   : {}", summary.skipped.len());
    if summary.aborted > 0 {
        println!("Not tried : {} (git could not be started)", summary.aborted);
    }
    println!("Elapsed   : {}", format_duration(elapsed));

    print_failures(&summary.failures);
    print_skipped(&summary.skipped);
    print_manual_commands(&summary.failures);
}

/// Print every failed repository with the classified cause and a manual command.
fn print_failures(failures: &[PullFailure]) {
    if failures.is_empty() {
        return;
    }

    println!();
    println!(
        "Failed ({}) - fix the cause, or pull these repositories manually:",
        failures.len()
    );
    for (index, failure) in failures.iter().enumerate() {
        println!("  {}. {}", index + 1, failure.path.display());
        println!("     remote : {} -> {}", failure.remote, failure.url);
        if let Some(branch) = &failure.branch {
            println!("     branch : {branch}");
        }
        println!("     status : {}", failure.status);
        println!("     cause  : {}", failure.kind.label());
        println!("     hint   : {}", failure.kind.hint());
        println!("     manual : {}", failure.manual);

        if let Some(output) = &failure.output {
            let lines = tail_lines(output, FAILURE_OUTPUT_LINES);
            if !lines.is_empty() {
                println!("     output :");
                for line in lines {
                    println!("       {line}");
                }
            }
        }
    }
}

/// Print every repository that matched but was not pulled.
fn print_skipped(skipped: &[SkippedRepo]) {
    if skipped.is_empty() {
        return;
    }

    println!();
    println!("Skipped ({}):", skipped.len());
    for (index, entry) in skipped.iter().enumerate() {
        println!(
            "  {}. {} - {}",
            index + 1,
            entry.path.display(),
            entry.reason
        );
    }
}

/// Print the failed pulls as ready-to-copy commands.
fn print_manual_commands(failures: &[PullFailure]) {
    if failures.is_empty() {
        return;
    }

    println!();
    println!("Manual pull commands:");
    for failure in failures {
        println!("  {}", failure.manual);
    }
}

/// Build the command a user can run by hand to retry one repository's pull.
fn manual_pull_command(path: &Path, extra: &[String]) -> String {
    let mut command = format!("git -C \"{}\" pull --no-edit", path.display());
    for arg in extra {
        command.push(' ');
        command.push_str(arg);
    }
    command
}

/// Last `max` non-empty lines of `text`, so the failure summary stays readable
/// while still carrying git's own error message.
fn tail_lines(text: &str, max: usize) -> Vec<String> {
    let lines: Vec<String> = text
        .split(['\r', '\n'])
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let start = lines.len().saturating_sub(max);
    lines[start..].to_vec()
}

/// Recursively collect every git repository root under `root`.
///
/// A directory is a repository root when it contains a `.git` entry; that also
/// covers linked worktrees and submodules, where `.git` is a file. Repositories
/// are not treated as leaves: walking continues so nested checkouts are found,
/// and `.gitignore` rules keep the traversal cheap.
fn find_git_repos(root: &Path) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    if is_git_root(root) {
        repos.push(root.to_path_buf());
    }

    for entry in WalkBuilder::new(root)
        .follow_links(false)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .filter_entry(|entry| entry.depth() == 0 || !should_skip_entry(entry))
        .build()
        .filter_map(|entry| entry.ok())
    {
        if entry.depth() == 0 {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir && is_git_root(entry.path()) {
            repos.push(entry.path().to_path_buf());
        }
    }

    repos.sort();
    repos.dedup();
    repos
}

/// Whether `path` is the working tree root of a git repository.
fn is_git_root(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Whether a walk entry is a directory that must not be crawled.
fn should_skip_entry(entry: &ignore::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
        && should_skip_dir(&entry.file_name().to_string_lossy())
}

/// Whether a directory name is a crawl-excluded cache directory.
fn should_skip_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

/// Open the repository at `path` and find the remote whose URL contains the
/// keyword. Returns `Ok(None)` when the repository has no such remote.
fn open_and_match(path: &Path, keyword_lower: &str) -> Result<Option<RemoteMatch>, git2::Error> {
    let repo = Repository::open(path)?;
    let branch = current_branch(&repo);
    let tracked = branch
        .as_deref()
        .and_then(|name| tracked_remote(&repo, name));

    let Some((remote, url)) = matching_remote(&repo, keyword_lower, tracked.as_deref()) else {
        return Ok(None);
    };

    let upstream_matches = tracked.as_deref() == Some(remote.as_str());

    Ok(Some(RemoteMatch {
        path: path.to_path_buf(),
        remote,
        url,
        branch,
        upstream_matches,
    }))
}

/// The checked out branch, or `None` for a detached HEAD and an unborn branch.
fn current_branch(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    head.shorthand().ok().map(str::to_string)
}

/// The remote configured as the upstream of `branch`, if any.
fn tracked_remote(repo: &Repository, branch: &str) -> Option<String> {
    let config = repo.config().ok()?;
    let remote = config.get_string(&format!("branch.{branch}.remote")).ok()?;
    if remote.is_empty() || remote == "." {
        None
    } else {
        Some(remote)
    }
}

/// Find the remote whose URL contains the keyword, preferring `preferred`
/// (the branch upstream) when it matches too.
fn matching_remote(
    repo: &Repository,
    keyword_lower: &str,
    preferred: Option<&str>,
) -> Option<(String, String)> {
    if let Some(found) = preferred.and_then(|name| remote_match(repo, name, keyword_lower)) {
        return Some(found);
    }

    let remotes = repo.remotes().ok()?;
    for name in remotes.iter().filter_map(|name| name.ok().flatten()) {
        if Some(name) == preferred {
            continue;
        }
        if let Some(found) = remote_match(repo, name, keyword_lower) {
            return Some(found);
        }
    }

    None
}

/// The `(name, url)` of a single remote when its URL contains the keyword.
fn remote_match(repo: &Repository, name: &str, keyword_lower: &str) -> Option<(String, String)> {
    let remote = repo.find_remote(name).ok()?;
    let url = remote.url().ok()?;
    if remote_url_matches(url, keyword_lower) {
        Some((name.to_string(), url.to_string()))
    } else {
        None
    }
}

/// Case-insensitive substring test between a remote URL and the keyword.
///
/// A blank keyword matches nothing: the handler rejects it before scanning,
/// and the guard keeps the matcher safe on its own.
fn remote_url_matches(url: &str, keyword_lower: &str) -> bool {
    let keyword = keyword_lower.trim();
    !keyword.is_empty() && url.to_lowercase().contains(keyword)
}

/// Extra `git pull` arguments after `--no-edit`.
///
/// When the branch upstream is the matching remote, a plain `git pull` is
/// enough and respects the user's pull configuration. Otherwise the matching
/// remote and the current branch are named explicitly, so repositories whose
/// upstream points elsewhere are not pulled from the wrong place.
fn pull_args(upstream_matches: bool, remote: &str, branch: Option<&str>) -> Option<Vec<String>> {
    if upstream_matches {
        return Some(Vec::new());
    }
    branch.map(|name| vec![remote.to_string(), name.to_string()])
}

/// Build `git -C <path> pull --no-edit` plus the extra remote/branch arguments.
fn git_pull_command(path: &Path, extra: &[String]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(path)
        .arg("pull")
        .arg("--no-edit")
        .args(extra);
    command
}

/// Run `git -C <path> pull --no-edit` with inherited standard streams, so git's
/// own progress, merge output and credential prompts reach the terminal.
fn run_git_pull(path: &Path, extra: &[String]) -> std::io::Result<std::process::ExitStatus> {
    git_pull_command(path, extra)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
}

/// Run a pull with captured output for concurrent execution. Standard input is
/// closed and terminal prompts are disabled so several git processes cannot
/// fight over the terminal; use `--jobs 1` for interactive authentication.
fn run_git_pull_captured(path: &Path, extra: &[String]) -> std::io::Result<Output> {
    git_pull_command(path, extra)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
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

    #[test]
    fn remote_url_matches_is_case_insensitive() {
        assert!(remote_url_matches(
            "https://github.com/lsk569937453/rcurl.git",
            "lsk569937453"
        ));
        assert!(remote_url_matches(
            "https://github.com/LSK569937453/rcurl.git",
            "lsk569937453"
        ));
        assert!(remote_url_matches(
            "git@github.com:lsk569937453/rcurl.git",
            "lsk569937453"
        ));
    }

    #[test]
    fn remote_url_matches_rejects_other_owners_and_blank_keywords() {
        assert!(!remote_url_matches(
            "https://github.com/someone-else/rcurl.git",
            "lsk569937453"
        ));
        assert!(!remote_url_matches(
            "https://github.com/lsk569937453/rcurl.git",
            "   "
        ));
    }

    #[test]
    fn pull_args_prefers_tracked_upstream() {
        assert_eq!(pull_args(true, "origin", Some("main")), Some(Vec::new()));
    }

    #[test]
    fn pull_args_names_remote_and_branch_when_upstream_differs() {
        assert_eq!(
            pull_args(false, "upstream", Some("dev")),
            Some(vec!["upstream".to_string(), "dev".to_string()])
        );
    }

    #[test]
    fn pull_args_skips_without_a_branch() {
        assert_eq!(pull_args(false, "origin", None), None);
    }

    #[test]
    fn should_skip_dir_recognizes_cache_dirs() {
        assert!(should_skip_dir("node_modules"));
        assert!(should_skip_dir("target"));
        assert!(should_skip_dir(".git"));
        assert!(!should_skip_dir("github"));
        assert!(!should_skip_dir("rust-projects"));
    }

    #[test]
    fn effective_workers_clamps_to_workload() {
        assert_eq!(effective_workers(8, 3), 3);
        assert_eq!(effective_workers(4, 10), 4);
        assert_eq!(effective_workers(1, 10), 1);
        assert_eq!(effective_workers(4, 0), 1);
        assert_eq!(effective_workers(0, 5), 1);
    }

    #[test]
    fn git_pull_command_builds_expected_arguments() {
        let command = git_pull_command(
            Path::new("repo"),
            &["upstream".to_string(), "dev".to_string()],
        );
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let expected: Vec<String> = ["-C", "repo", "pull", "--no-edit", "upstream", "dev"]
            .iter()
            .map(|arg| arg.to_string())
            .collect();
        assert_eq!(args, expected);
    }

    #[test]
    fn captured_output_text_combines_stdout_and_stderr() {
        assert_eq!(
            captured_output_text(b"Already up to date.", b"warning"),
            "Already up to date.\nwarning"
        );
        assert_eq!(captured_output_text(b"line\n", b"error"), "line\nerror");
        assert_eq!(captured_output_text(b"", b"only stderr"), "only stderr");
    }

    #[test]
    fn captured_output_text_handles_invalid_utf8() {
        let text = captured_output_text(&[0xff], b"");
        assert!(text.contains('\u{fffd}'));
    }

    #[test]
    fn failure_kind_classifies_common_git_errors() {
        assert_eq!(
            FailureKind::from_output(
                "fatal: could not read Username for 'https://github.com': terminal prompts disabled"
            ),
            FailureKind::Authentication
        );
        assert_eq!(
            FailureKind::from_output(
                "error: Your local changes to the following files would be overwritten by merge:"
            ),
            FailureKind::LocalChanges
        );
        assert_eq!(
            FailureKind::from_output("CONFLICT (content): Merge conflict in src/main.rs"),
            FailureKind::Conflict
        );
        assert_eq!(
            FailureKind::from_output("fatal: Need to specify how to reconcile divergent branches."),
            FailureKind::Diverged
        );
        assert_eq!(
            FailureKind::from_output("fatal: couldn't find remote ref main"),
            FailureKind::RemoteMissing
        );
        assert_eq!(
            FailureKind::from_output(
                "Your configuration specifies to merge with the ref 'refs/heads/main'\nfrom the remote, but no such ref was fetched."
            ),
            FailureKind::RemoteMissing
        );
        assert_eq!(
            FailureKind::from_output(
                "fatal: unable to access 'https://github.com/x/y.git/': Could not resolve host: github.com"
            ),
            FailureKind::Network
        );
        assert_eq!(
            FailureKind::from_output("fatal: something entirely different"),
            FailureKind::Other
        );
    }

    #[test]
    fn failure_kind_prefers_the_most_specific_cause() {
        // A 403 arrives wrapped in a transport message; the credential cause wins.
        assert_eq!(
            FailureKind::from_output(
                "fatal: unable to access 'https://github.com/x/y.git/': The requested URL returned error: 403"
            ),
            FailureKind::Authentication
        );
    }

    #[test]
    fn manual_pull_command_quotes_the_path_and_keeps_extra_args() {
        assert_eq!(
            manual_pull_command(
                Path::new("C:\\code\\a b"),
                &["upstream".to_string(), "dev".to_string()]
            ),
            "git -C \"C:\\code\\a b\" pull --no-edit upstream dev"
        );
        assert_eq!(
            manual_pull_command(Path::new("repo"), &[]),
            "git -C \"repo\" pull --no-edit"
        );
    }

    #[test]
    fn tail_lines_keeps_the_trailing_non_empty_lines() {
        assert_eq!(tail_lines("a\n\nb\r\nc\n", 2), vec!["b", "c"]);
        assert_eq!(tail_lines("", 5), Vec::<String>::new());
        assert_eq!(tail_lines("a\nb", 10), vec!["a", "b"]);
    }
}
