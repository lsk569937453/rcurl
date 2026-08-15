use crate::git::model::{
    to_local_time, CommitAgg, CommitTaskResult, FileExtensionItem, FileStatisticInfo,
    GitBaseInfo, GitStatistics, TagItem, TagStatisticInfo,
};
use anyhow::anyhow;
use git2::{Delta, DiffFormat, DiffOptions, Repository, TreeWalkMode};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn analyze(repo_dir: &Path) -> Result<GitStatistics, anyhow::Error> {
    let repo = Repository::open(repo_dir)?;

    // Tracked file count from the index; bare repositories fall back to the HEAD tree.
    let total_files = match repo.index() {
        Ok(index) => index.len() as i64,
        Err(_) => repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .and_then(|c| c.tree())
            .map(|t| tree_blob_count(&repo, &t))
            .unwrap_or(0),
    };

    // All commits reachable from HEAD, oldest first (upstream reverses the walk).
    let mut revwalk = repo
        .revwalk()
        .map_err(|_| anyhow!("empty repository: no commits on HEAD"))?;
    revwalk
        .push_head()
        .map_err(|_| anyhow!("empty repository: no commits on HEAD"))?;
    let mut oids: Vec<git2::Oid> = revwalk.collect::<Result<Vec<_>, _>>()?;
    if oids.is_empty() {
        return Err(anyhow!("empty repository: no commits on HEAD"));
    }
    oids.reverse();

    // Mainline Oids (like --first-parent): only these count toward line totals.
    let mainline = mainline_oids(repo_dir)?;

    // Tags first, so the progress bar covers both phases.
    let tag_refs = collect_tag_refs(&repo)?;

    let pb = ProgressBar::new((oids.len() + tag_refs.len()) as u64);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{pos}/{len}] {msg}")?);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Per-commit diff analysis in parallel; each task opens its own Repository
    // (libgit2 Repository is not Send).
    let done = AtomicUsize::new(0);
    let total_commits = oids.len();
    let results: Vec<CommitTaskResult> = oids
        .par_iter()
        .map(|oid| -> Result<CommitTaskResult, anyhow::Error> {
            let task_repo = Repository::open(repo_dir)?;
            let result = diff_stats(&task_repo, *oid, mainline.contains(oid));
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            pb.set_message(format!("Analyzed [{}/{}] commits", n, total_commits));
            pb.inc(1);
            result
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Sequential fold keeps aggregation deterministic.
    let mut agg = CommitAgg::default();
    let mut authors: HashSet<String> = HashSet::new();
    let mut total_added = 0i64;
    let mut total_deleted = 0i64;
    for r in &results {
        authors.insert(r.author.clone());
        if r.is_mainline {
            total_added += r.added;
            total_deleted += r.deleted;
        }
        agg.fold(r);
    }

    let first_commit = repo.find_commit(oids[0])?;
    let last_commit = repo.find_commit(*oids.last().unwrap())?;
    let first_time = first_commit.time().seconds();
    let last_time = last_commit.time().seconds();

    pb.set_message("Analyzing HEAD tree");
    let file_statistic_info = analyze_files(&repo)?;

    let tag_statistic_info = analyze_tags(repo_dir, &tag_refs, &pb, total_commits)?;
    pb.finish_with_message("Git analysis complete!");

    let (commit_info, author_statistic_info, loc_raw) = agg.into_parts();
    let line_statistic_info = loc_raw.into_series();
    let age = last_time / 86400 - first_time / 86400 + 1;

    let project_name = repo_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repository")
        .to_string();

    Ok(GitStatistics {
        git_base_info: GitBaseInfo {
            project_name,
            generate_time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            age,
            active_days: age,
            total_files,
            total_lines: line_statistic_info.total_lines,
            total_added,
            total_deleted,
            total_commits: results.len() as i64,
            authors: authors.len() as i64,
            first_commit_time: to_local_time(first_time).format("%Y-%m-%d %H:%M:%S").to_string(),
            last_commit_time: to_local_time(last_time).format("%Y-%m-%d %H:%M:%S").to_string(),
        },
        commit_info,
        author_statistic_info,
        file_statistic_info,
        line_statistic_info,
        tag_statistic_info,
    })
}

fn tree_blob_count(_repo: &Repository, tree: &git2::Tree) -> i64 {
    let mut count = 0i64;
    let _ = tree.walk(TreeWalkMode::PreOrder, |_, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            count += 1;
        }
        git2::TreeWalkResult::Ok
    });
    count
}

fn mainline_oids(repo_dir: &Path) -> Result<HashSet<git2::Oid>, anyhow::Error> {
    let repo = Repository::open(repo_dir)?;
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.simplify_first_parent()?;
    let mut set = HashSet::new();
    for oid in revwalk {
        set.insert(oid?);
    }
    Ok(set)
}

/// Diff one commit against its first parent; counts +/- lines and per-file
/// (added, deleted) for Added/Modified/Deleted deltas.
fn diff_stats(
    repo: &Repository,
    oid: git2::Oid,
    is_mainline: bool,
) -> Result<CommitTaskResult, anyhow::Error> {
    let commit = repo
        .find_commit(oid)
        .map_err(|e| anyhow!("can not find commit: {}", e))?;
    let first_parent_tree = if let Ok(parent) = commit.parent(0) {
        Some(parent.tree()?)
    } else {
        None
    };
    let author_name = commit.author().name().unwrap_or("Unknown").to_string();
    let tree = commit.tree()?;
    let mut opts = DiffOptions::new();
    let mut diff =
        repo.diff_tree_to_tree(first_parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;
    diff.find_similar(None)?;

    let mut added = 0i64;
    let mut deleted = 0i64;
    let mut files: HashMap<String, (i64, i64)> = HashMap::new();
    diff.print(DiffFormat::Patch, |delta, _hunk, line| {
        match delta.status() {
            Delta::Added | Delta::Modified | Delta::Deleted => {
                if let Some(path) = delta.new_file().path() {
                    let filename = path.display().to_string();
                    let line_added = i64::from(line.origin() == '+');
                    let line_deleted = i64::from(line.origin() == '-');
                    added += line_added;
                    deleted += line_deleted;
                    let entry = files.entry(filename).or_insert((0, 0));
                    entry.0 += line_added;
                    entry.1 += line_deleted;
                }
            }
            _ => {}
        }
        true
    })?;

    Ok(CommitTaskResult {
        time: to_local_time(commit.time().seconds()),
        author: author_name,
        added,
        deleted,
        is_mainline,
        files,
    })
}

/// Extension statistics over the HEAD tree (upstream analyze_files).
fn analyze_files(repo: &Repository) -> Result<FileStatisticInfo, anyhow::Error> {
    let head_commit = repo.head()?.peel_to_commit()?;
    let tree = head_commit.tree()?;

    let mut total_size = 0i64;
    let mut total_files = 0i64;
    let mut ext_map: HashMap<String, FileExtensionItem> = HashMap::new();

    tree.walk(TreeWalkMode::PreOrder, |_, entry| {
        if let Some(blob) = entry.to_object(repo).ok().and_then(|o| o.as_blob().cloned()) {
            total_size += blob.size() as i64;
            total_files += 1;

            let full_name = entry.name().unwrap_or_default();
            let filename = full_name.rsplit('/').next().unwrap_or_default();
            let ext = match filename.rfind('.') {
                // Hidden files (".gitignore") and over-long extensions count as none.
                Some(0) | None => String::new(),
                Some(idx) => {
                    let ext = filename[idx + 1..].to_string();
                    if ext.len() > 10 {
                        String::new()
                    } else {
                        ext
                    }
                }
            };
            let line_count = blob.content().split(|&c| c == b'\n').count() as i64;
            let item = ext_map.entry(ext.clone()).or_insert(FileExtensionItem {
                extention_name: ext,
                files_count: 0,
                lines_count: 0,
            });
            item.files_count += 1;
            item.lines_count += line_count;
        }
        git2::TreeWalkResult::Ok
    })?;

    let average_file_size = if total_files > 0 {
        format!("{:.2}", total_size as f64 / total_files as f64)
    } else {
        "0.00".to_string()
    };

    let mut extensions: Vec<FileExtensionItem> = ext_map.into_values().collect();
    extensions.sort_by_key(|e| std::cmp::Reverse(e.files_count));

    Ok(FileStatisticInfo {
        total_files_count: total_files,
        total_lines_count: 0,
        average_file_size,
        extensions,
    })
}

struct TagRef {
    tag_name: String,
    tag_oid: git2::Oid,
    date_time: chrono::DateTime<chrono::Utc>,
}

/// Collect tag refs: annotated tags use the tagger time (falling back to the
/// peeled commit time), lightweight tags use the commit time.
fn collect_tag_refs(repo: &Repository) -> Result<Vec<TagRef>, anyhow::Error> {
    let mut tag_refs = Vec::new();
    for r in repo.references()? {
        let r = r?;
        if !r.is_tag() {
            continue;
        }
        let Ok(name) = r.shorthand() else { continue };
        let Some(target) = r.target() else { continue };
        let when = if let Ok(tag) = repo.find_tag(target) {
            tag.tagger()
                .map(|t| t.when())
                .or_else(|| repo.find_commit(tag.target_id()).ok().map(|c| c.time()))
                .unwrap_or_else(|| git2::Time::new(0, 0))
        } else {
            repo.find_commit(target)
                .map(|c| c.time())
                .unwrap_or_else(|_| git2::Time::new(0, 0))
        };
        let date_time = chrono::DateTime::from_timestamp(when.seconds(), 0)
            .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).expect("epoch"));
        tag_refs.push(TagRef {
            tag_name: name.to_string(),
            tag_oid: target,
            date_time,
        });
    }
    tag_refs.sort_by_key(|t| t.date_time);
    Ok(tag_refs)
}

/// Per-tag commit/author counts over the interval between adjacent tags
/// (revwalk.push(tag) + hide(previous tag)).
fn analyze_tags(
    repo_dir: &Path,
    tag_refs: &[TagRef],
    pb: &ProgressBar,
    base_done: usize,
) -> Result<TagStatisticInfo, anyhow::Error> {
    let done = AtomicUsize::new(0);
    let total = tag_refs.len();
    let list: Vec<TagItem> = tag_refs
        .par_iter()
        .enumerate()
        .map(|(idx, tag)| -> Result<TagItem, anyhow::Error> {
            let repo = Repository::open(repo_dir)?;
            let mut revwalk = repo.revwalk()?;
            revwalk.push(tag.tag_oid)?;
            if idx > 0 {
                revwalk.hide(tag_refs[idx - 1].tag_oid)?;
            }
            let mut commit_count = 0i64;
            let mut author_count: HashMap<String, i64> = HashMap::new();
            for oid in revwalk {
                let commit = repo.find_commit(oid?)?;
                let author = commit.author().name().unwrap_or("Unknown").to_string();
                commit_count += 1;
                *author_count.entry(author).or_insert(0) += 1;
            }
            let mut authors: Vec<(String, i64)> = author_count.into_iter().collect();
            // Descending by commit count (upstream sorted ascending by mistake).
            authors.sort_by_key(|a| std::cmp::Reverse(a.1));

            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            pb.set_message(format!("Analyzed [{}/{}] tags: {}", n, total, tag.tag_name));
            pb.set_position((base_done + n) as u64);
            Ok(TagItem {
                tag_name: tag.tag_name.clone(),
                date: tag.date_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                commit_count,
                authors,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let total_tags = list.len() as i64;
    let total_tag_commits: i64 = list.iter().map(|t| t.commit_count).sum();
    let average_commit_per_tag = if total_tags > 0 {
        format!("{:.2}", total_tag_commits as f64 / total_tags as f64)
    } else {
        "0.00".to_string()
    };

    let mut list = list;
    list.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(TagStatisticInfo {
        total_tags,
        average_commit_per_tag,
        list,
    })
}
