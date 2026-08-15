use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};

/// Per-commit analysis result produced in parallel, then folded in order.
pub struct CommitTaskResult {
    pub time: DateTime<Local>,
    pub author: String,
    pub added: i64,
    pub deleted: i64,
    /// Whether this commit is on the first-parent mainline (like --first-parent).
    pub is_mainline: bool,
    /// file path -> (added, deleted)
    pub files: HashMap<String, (i64, i64)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GitStatistics {
    pub git_base_info: GitBaseInfo,
    pub commit_info: CommitInfo,
    pub author_statistic_info: AuthorStatisticInfo,
    pub file_statistic_info: FileStatisticInfo,
    pub line_statistic_info: LineStatisticInfo,
    pub tag_statistic_info: TagStatisticInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GitBaseInfo {
    pub project_name: String,
    pub generate_time: String,
    pub age: i64,
    pub active_days: i64,
    pub total_files: i64,
    pub total_lines: i64,
    pub total_added: i64,
    pub total_deleted: i64,
    pub total_commits: i64,
    pub authors: i64,
    pub first_commit_time: String,
    pub last_commit_time: String,
}

/// Commit time distributions, each sorted and zero-filled, ready for charts.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CommitInfo {
    /// [weekIndex, count] where 0 = this week, up to 32.
    pub recent_weeks_commit: Vec<(i32, i64)>,
    /// [hour 0-23, count]
    pub hours_commit: Vec<(i32, i64)>,
    /// [day 1-7 (Monday = 1), count]
    pub day_of_week_commit: Vec<(i32, i64)>,
    /// [month 1-12, count]
    pub month_of_year_commit: Vec<(i32, i64)>,
    /// ["YYYY-MM", count] sorted
    pub year_and_month_commit: Vec<(String, i64)>,
    /// [year, count] sorted
    pub year_commit: Vec<(i32, i64)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AuthorStatisticInfo {
    /// Top 20 authors by total commits, descending.
    pub total_authors: Vec<AuthorTotalItem>,
    /// Monthly champions, date descending.
    pub author_of_month: Vec<AuthorPeriodChampion>,
    /// Yearly champions, date descending.
    pub author_of_year: Vec<AuthorPeriodChampion>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthorTotalItem {
    pub author_name: String,
    pub total_commit: i64,
    pub total_added: i64,
    pub total_deleted: i64,
    pub first_commit: String,
    pub last_commit: String,
    pub age: i64,
    pub active_days: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthorPeriodChampion {
    pub date: String,
    pub author_name: String,
    pub count_of_commit_of_author: i64,
    pub total_commit_count: i64,
    pub count_of_author: i64,
    pub next_top_five: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FileStatisticInfo {
    pub total_files_count: i64,
    pub total_lines_count: i64,
    pub average_file_size: String,
    /// Extensions by files_count descending.
    pub extensions: Vec<FileExtensionItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileExtensionItem {
    pub extention_name: String,
    pub files_count: i64,
    pub lines_count: i64,
}

/// Cumulative LOC evolution (mainline commits only).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LineStatisticInfo {
    pub total_lines: i64,
    /// Zero-filled consecutive dates "YYYY-MM-DD 00:00:00", ascending.
    pub dates: Vec<String>,
    /// Cumulative sum aligned with `dates`.
    pub total_cumulative: Vec<i64>,
    /// Per-directory cumulative series, sorted by dir name.
    pub dirs: Vec<DirLocSeries>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DirLocSeries {
    pub dir_name: String,
    /// Cumulative sum aligned with `dates`.
    pub cumulative: Vec<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TagStatisticInfo {
    pub total_tags: i64,
    pub average_commit_per_tag: String,
    /// Tags by date descending.
    pub list: Vec<TagItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TagItem {
    pub tag_name: String,
    pub date: String,
    pub commit_count: i64,
    /// [author, commit count] by count descending.
    pub authors: Vec<(String, i64)>,
}

/// Intermediate accumulation state (mirrors the upstream GitStatisticInfo maps).
#[derive(Default)]
pub struct CommitAgg {
    pub recent_weeks: HashMap<i32, i64>,
    pub hours: HashMap<i32, i64>,
    pub day_of_week: HashMap<i32, i64>,
    pub month_of_year: HashMap<i32, i64>,
    pub year_and_month: HashMap<String, i64>,
    pub year: HashMap<i32, i64>,
    /// author -> per-author totals
    pub authors: HashMap<String, AuthorTotalItem>,
    /// "YYYY-MM" -> author -> commit count
    pub authors_of_month: HashMap<String, HashMap<String, i64>>,
    /// "YYYY" -> author -> commit count
    pub authors_of_year: HashMap<String, HashMap<String, i64>>,
    /// date "YYYY-MM-DD 00:00:00" -> net added lines (mainline only)
    pub loc_by_day: HashMap<String, i64>,
    /// dir -> date -> net added lines (mainline only)
    pub loc_by_dir: HashMap<String, HashMap<String, i64>>,
    pub total_lines: i64,
}

impl CommitAgg {
    /// Fold one commit into the aggregation (equivalent of upstream calc_commit).
    pub fn fold(&mut self, r: &CommitTaskResult) {
        let time = &r.time;
        self.calc_recent_week(time);
        self.hours.entry(time.hour() as i32).and_modify(|c| *c += 1).or_insert(1);
        self.day_of_week
            .entry(time.date_naive().weekday().number_from_monday() as i32)
            .and_modify(|c| *c += 1)
            .or_insert(1);
        self.month_of_year
            .entry(time.month() as i32)
            .and_modify(|c| *c += 1)
            .or_insert(1);
        let ym = time.format("%Y-%m").to_string();
        self.year_and_month.entry(ym.clone()).and_modify(|c| *c += 1).or_insert(1);
        self.year.entry(time.year()).and_modify(|c| *c += 1).or_insert(1);

        self.calc_author(time, &r.author, r.added, r.deleted);
        self.calc_period_authors(&ym, &r.author);
        let y = time.format("%Y").to_string();
        let year_authors = self.authors_of_year.entry(y).or_default();
        *year_authors.entry(r.author.clone()).or_insert(0) += 1;

        if r.is_mainline {
            self.calc_lines_of_code(time, r.added, r.deleted, &r.files);
        }
    }

    fn calc_recent_week(&mut self, time: &DateTime<Local>) {
        let week = time.iso_week().week() as i32;
        let year = time.iso_week().year();
        let now = Utc::now();
        let now_week = now.iso_week().week() as i32;
        let now_year = now.iso_week().year();
        let week_number = (now_year * 52 + now_week) - (year * 52 + week);
        if (0..=32).contains(&week_number) {
            self.recent_weeks
                .entry(week_number)
                .and_modify(|c| *c += 1)
                .or_insert(1);
        }
    }

    fn calc_author(&mut self, time: &DateTime<Local>, author: &str, added: i64, deleted: i64) {
        let commit_time = time.format("%Y-%m-%d").to_string();
        let entry = self.authors.entry(author.to_string()).or_insert_with(|| AuthorTotalItem {
            author_name: author.to_string(),
            total_commit: 0,
            total_added: 0,
            total_deleted: 0,
            first_commit: commit_time.clone(),
            last_commit: commit_time.clone(),
            age: 0,
            active_days: 0,
        });
        entry.total_commit += 1;
        entry.total_added += added;
        entry.total_deleted += deleted;
        entry.active_days += 1;
        if commit_time < entry.first_commit {
            entry.first_commit = commit_time.clone();
        }
        if commit_time > entry.last_commit {
            entry.last_commit = commit_time;
        }
        if let (Ok(d1), Ok(d2)) = (
            NaiveDate::parse_from_str(&entry.last_commit, "%Y-%m-%d"),
            NaiveDate::parse_from_str(&entry.first_commit, "%Y-%m-%d"),
        ) {
            entry.age = d1.signed_duration_since(d2).num_days();
        }
    }

    fn calc_period_authors(&mut self, ym: &str, author: &str) {
        let month_authors = self.authors_of_month.entry(ym.to_string()).or_default();
        *month_authors.entry(author.to_string()).or_insert(0) += 1;
    }

    fn calc_lines_of_code(
        &mut self,
        time: &DateTime<Local>,
        added: i64,
        deleted: i64,
        files: &HashMap<String, (i64, i64)>,
    ) {
        let total = added - deleted;
        self.total_lines += total;
        let day = time.format("%Y-%m-%d 00:00:00").to_string();
        *self.loc_by_day.entry(day.clone()).or_insert(0) += total;
        for (file_name, (file_added, file_deleted)) in files {
            for dir in get_dirs(file_name) {
                let dir_map = self.loc_by_dir.entry(dir).or_default();
                *dir_map.entry(day.clone()).or_insert(0) += file_added - file_deleted;
            }
        }
    }

    /// Convert accumulated maps into the final serializable model.
    pub fn into_parts(self) -> (CommitInfo, AuthorStatisticInfo, LineStatisticRaw) {
        let mut year_commit: Vec<(i32, i64)> =
            self.year.into_iter().collect();
        year_commit.sort_by_key(|e| e.0);
        let commit_info = CommitInfo {
            recent_weeks_commit: sorted_filled_i32(self.recent_weeks, 0, 32),
            hours_commit: sorted_filled_i32(self.hours, 0, 23),
            day_of_week_commit: sorted_filled_i32(self.day_of_week, 1, 7),
            month_of_year_commit: sorted_filled_i32(self.month_of_year, 1, 12),
            year_and_month_commit: sorted_string(self.year_and_month),
            year_commit,
        };

        let mut total_authors: Vec<AuthorTotalItem> = self.authors.values().cloned().collect();
        total_authors.sort_by_key(|a| std::cmp::Reverse(a.total_commit));
        total_authors.truncate(20);

        let author_statistic_info = AuthorStatisticInfo {
            total_authors,
            author_of_month: champions_from(self.authors_of_month),
            author_of_year: champions_from(self.authors_of_year),
        };

        let raw = LineStatisticRaw {
            loc_by_day: self.loc_by_day,
            loc_by_dir: self.loc_by_dir,
            total_lines: self.total_lines,
        };

        (commit_info, author_statistic_info, raw)
    }
}

/// Raw LOC maps before zero-filling/cumulative-sum, kept separate so the date
/// range can be shared between the total series and per-directory series.
pub struct LineStatisticRaw {
    pub loc_by_day: HashMap<String, i64>,
    pub loc_by_dir: HashMap<String, HashMap<String, i64>>,
    pub total_lines: i64,
}

impl LineStatisticRaw {
    /// Zero-fill the date range, sort, and cumulative-sum, producing the final
    /// LOC series. Directory series share the total series' date range.
    pub fn into_series(self) -> LineStatisticInfo {
        let default_day = NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid date");
        let parse_day = |key: &String| {
            key.get(..10)
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .unwrap_or(default_day)
        };
        let start = self
            .loc_by_day
            .keys()
            .min()
            .map(parse_day)
            .unwrap_or(default_day)
            - chrono::Duration::days(1);
        let end = self
            .loc_by_day
            .keys()
            .max()
            .map(parse_day)
            .unwrap_or(default_day)
            + chrono::Duration::days(1);

        let mut dates = Vec::new();
        let mut total_cumulative = Vec::new();
        let mut running: i64 = 0;
        let mut cursor = start;
        while cursor <= end {
            let key = format!("{} 00:00:00", cursor.format("%Y-%m-%d"));
            running += self.loc_by_day.get(&key).copied().unwrap_or(0);
            dates.push(key);
            total_cumulative.push(running);
            cursor += chrono::Duration::days(1);
        }

        let mut dirs: Vec<DirLocSeries> = self
            .loc_by_dir
            .into_iter()
            .map(|(dir, by_day)| {
                let mut running = 0i64;
                let cumulative = dates
                    .iter()
                    .map(|d| {
                        running += by_day.get(d).copied().unwrap_or(0);
                        running
                    })
                    .collect();
                DirLocSeries { dir_name: dir, cumulative }
            })
            .collect();
        dirs.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));

        LineStatisticInfo {
            total_lines: self.total_lines,
            dates,
            total_cumulative,
            dirs,
        }
    }
}

/// Build the champion list from a period -> author -> count map (upstream
/// AuthorOfMonthResponse::from_hashmap).
fn champions_from(map: HashMap<String, HashMap<String, i64>>) -> Vec<AuthorPeriodChampion> {
    let mut data = Vec::new();
    for (date, authors) in map {
        let mut total_commit_count = 0i64;
        let count_of_author = authors.len() as i64;
        let mut heap: BinaryHeap<(i64, String)> = authors
            .into_iter()
            .map(|(author, commit)| {
                total_commit_count += commit;
                (commit, author)
            })
            .collect();
        let (author_name, count_of_commit_of_author) =
            heap.pop().map(|(c, a)| (a, c)).unwrap_or_default();
        let next_top_five: Vec<String> =
            std::iter::from_fn(|| heap.pop().map(|(_, a)| a)).take(5).collect();
        data.push(AuthorPeriodChampion {
            date,
            author_name,
            count_of_commit_of_author,
            total_commit_count,
            count_of_author,
            next_top_five,
        });
    }
    data.sort_by(|a, b| b.date.cmp(&a.date));
    data
}

/// Sort a numeric histogram and zero-fill the full [lo, hi] key range, so charts
/// show empty buckets as 0 instead of dropping them.
fn sorted_filled_i32(map: HashMap<i32, i64>, lo: i32, hi: i32) -> Vec<(i32, i64)> {
    let mut out = Vec::with_capacity(hi.saturating_sub(lo).max(0) as usize + 1);
    for k in lo..=hi {
        out.push((k, map.get(&k).copied().unwrap_or(0)));
    }
    out
}

fn sorted_string(map: HashMap<String, i64>) -> Vec<(String, i64)> {
    let mut entries: Vec<(String, i64)> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

/// Directory prefixes of a repo-relative file path: "src/a/b.rs" -> ["src", "src/a"].
pub fn get_dirs(input: &str) -> Vec<String> {
    let mut results = Vec::new();
    let splits: Vec<&str> = input.split('/').collect();
    if splits.len() <= 1 {
        return results;
    }
    let mut temp = splits[0].to_string();
    results.push(temp.clone());
    for part in &splits[1..splits.len() - 1] {
        let current = format!("{}/{}", temp, part);
        results.push(current.clone());
        temp = current;
    }
    results
}

/// Convert a git2 epoch timestamp to local time.
pub fn to_local_time(secs: i64) -> DateTime<Local> {
    use chrono::TimeZone;
    Utc
        .timestamp_opt(secs, 0)
        .single()
        .map(DateTime::<Local>::from)
        .unwrap_or_else(Local::now)
}
