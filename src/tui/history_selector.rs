use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::ops::Range;
use std::time::Duration;

use crate::history::types::HistoryEntry;

/// 历史选择界面的焦点：命令列表或检索输入框。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    List,
    Search,
}

pub struct HistorySelector {
    entries: Vec<HistoryEntry>,
    /// 命中当前检索条件的条目在 `entries` 中的下标。
    filtered: Vec<usize>,
    /// 检索条件，按空白分词后要求全部命中。
    query: String,
    focus: Focus,
    /// `filtered` 中的选中位置。
    selected: usize,
    should_exit: bool,
    /// 仅当用户用 Enter 确认选中项时为 true，避免退出时误把命令注入 shell。
    confirmed: bool,
    list_state: ListState,
}

impl HistorySelector {
    pub fn new(entries: Vec<HistoryEntry>) -> Self {
        let filtered: Vec<usize> = (0..entries.len()).collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            entries,
            filtered,
            query: String::new(),
            focus: Focus::List,
            selected: 0,
            should_exit: false,
            confirmed: false,
            list_state,
        }
    }

    pub fn run(&mut self) -> Result<Option<String>> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_app(&mut terminal);

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    fn run_app(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<Option<String>> {
        while !self.should_exit {
            terminal.draw(|f| {
                // Create a mutable reference for the UI
                let selector = &mut *self;
                selector.ui(f)
            })?;

            // Check for events with timeout
            if event::poll(Duration::from_millis(100))? {
                if let event::Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_keypress(key)?;
                    }
                }
            }
        }

        Ok(self.outcome())
    }

    /// 只有按 Enter 确认过选中项时才返回命令；q/Esc/Ctrl+C 退出返回 None。
    fn outcome(&self) -> Option<String> {
        if self.confirmed {
            self.selected_command()
        } else {
            None
        }
    }

    fn handle_keypress(&mut self, key: KeyEvent) -> Result<()> {
        // Ctrl+C 在任意状态下都直接取消，不注入任何命令。
        let is_ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char(c) if c.eq_ignore_ascii_case(&'c'));
        if is_ctrl_c {
            self.should_exit = true;
            return Ok(());
        }

        match self.focus {
            Focus::List => self.handle_list_keypress(key),
            Focus::Search => self.handle_search_keypress(key),
        }

        Ok(())
    }

    fn handle_list_keypress(&mut self, key: KeyEvent) {
        // 列表态只处理明确支持的按键，带 CONTROL/ALT 的组合一律忽略。
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_exit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Home => self.select_first(),
            KeyCode::End => self.select_last(),
            KeyCode::Enter => self.confirm_selection(),
            KeyCode::Char('/') => self.focus = Focus::Search,
            _ => {}
        }
    }

    fn handle_search_keypress(&mut self, key: KeyEvent) {
        // 检索态的 Ctrl 组合只支持 Ctrl+U（清空检索条件）。
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if matches!(key.code, KeyCode::Char(c) if c.eq_ignore_ascii_case(&'u')) {
                self.clear_query();
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            return;
        }

        match key.code {
            KeyCode::Esc => self.leave_search(),
            KeyCode::Enter => self.confirm_selection(),
            KeyCode::Backspace => {
                if self.query.pop().is_some() {
                    self.recompute_filter();
                }
            }
            // 检索态下字符（含 q/j/k）都作为检索条件输入，只用方向键移动选中项。
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Home => self.select_first(),
            KeyCode::End => self.select_last(),
            KeyCode::Char(c) => {
                self.query.push(c);
                self.recompute_filter();
            }
            _ => {}
        }
    }

    /// 依据当前检索条件重建命中列表，并把选中位置收敛到有效范围。
    fn recompute_filter(&mut self) {
        let terms = parse_terms(&self.query);
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches_terms(&entry.command, &terms))
            .map(|(index, _)| index)
            .collect();
        // 条件变化后回到首个命中项，避免选中项停在旧位置。
        self.selected = 0;
        self.sync_list_state();
    }

    fn sync_list_state(&mut self) {
        self.selected = self.selected.min(self.filtered_len().saturating_sub(1));
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(self.selected));
        }
    }

    fn filtered_len(&self) -> usize {
        self.filtered.len()
    }

    fn selected_entry(&self) -> Option<&HistoryEntry> {
        self.filtered
            .get(self.selected)
            .and_then(|index| self.entries.get(*index))
    }

    fn selected_command(&self) -> Option<String> {
        self.selected_entry().map(|entry| entry.command.clone())
    }

    fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let last = self.filtered.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.list_state.select(Some(self.selected));
    }

    fn select_first(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = 0;
        self.list_state.select(Some(0));
    }

    fn select_last(&mut self) {
        if let Some(last) = self.filtered.len().checked_sub(1) {
            self.selected = last;
            self.list_state.select(Some(last));
        }
    }

    /// 清空检索条件并恢复完整历史列表。
    fn clear_query(&mut self) {
        if self.query.is_empty() {
            return;
        }
        self.query.clear();
        self.recompute_filter();
    }

    /// Esc：检索态先清空条件并回到列表（不退出），再次按 Esc 才退出。
    fn leave_search(&mut self) {
        self.clear_query();
        self.focus = Focus::List;
    }

    fn confirm_selection(&mut self) {
        if self.selected_entry().is_some() {
            self.confirmed = true;
            self.should_exit = true;
        }
    }

    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .split(f.area());

        self.render_title(f, chunks[0]);
        self.render_search(f, chunks[1]);
        self.render_list(f, chunks[2]);
        self.render_footer(f, chunks[3]);
    }

    fn render_title(&self, f: &mut Frame, area: Rect) {
        let title = Paragraph::new(Line::from(vec![Span::styled(
            "Command History",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, area);
    }

    fn render_search(&self, f: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Search;
        let border_color = if focused { Color::Cyan } else { Color::DarkGray };
        // 去掉左右边框后可用于显示检索条件的宽度。
        let inner_width = usize::from(area.width.saturating_sub(2));

        let mut spans = Vec::new();
        if self.query.is_empty() && !focused {
            spans.push(Span::styled("Press / to search", Style::default().fg(Color::DarkGray)));
        } else {
            let prefix = "Search: ";
            // 检索条件过长时只显示尾部，避免光标被裁掉。
            let budget = inner_width.saturating_sub(prefix.chars().count() + 1);
            spans.push(Span::styled(prefix, Style::default().fg(Color::Gray)));
            spans.push(Span::styled(
                tail_chars(&self.query, budget),
                Style::default().fg(Color::White),
            ));
            if focused {
                spans.push(Span::styled("▌", Style::default().fg(Color::Cyan)));
            }
        }

        let search = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search ")
                .border_style(Style::default().fg(border_color)),
        );
        f.render_widget(search, area);
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect) {
        if self.entries.is_empty() {
            let empty_msg = Paragraph::new("No request history found. Run a command first to create history.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            f.render_widget(empty_msg, area);
            return;
        }

        if self.filtered.is_empty() {
            let no_match = Paragraph::new(
                "No matching requests. Backspace to edit, Esc to clear the search.",
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
            f.render_widget(no_match, area);
            return;
        }

        let terms = parse_terms(&self.query);
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(position, index)| {
                let entry = &self.entries[*index];
                let display_cmd = entry.display_terminal_command(100);
                let is_selected = position == self.selected;
                let base_style = if is_selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(build_line(&display_cmd, &terms, base_style, is_selected))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let status = if self.entries.is_empty() {
            "Press q/Esc to quit".to_string()
        } else if self.filtered.is_empty() {
            format!("0 matches · total {}", self.entries.len())
        } else {
            format!(
                "[{}/{}] · total {}",
                self.selected + 1,
                self.filtered.len(),
                self.entries.len()
            )
        };
        let hint = match self.focus {
            Focus::List => "↑/↓ move · / search · Enter run · q quit",
            Focus::Search => "type to filter · Backspace edit · Esc cancel",
        };

        let footer = Paragraph::new(Line::from(vec![
            Span::styled(status, Style::default().fg(Color::Gray)),
            Span::styled("  ", Style::default()),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(footer, area);
    }
}

/// 把检索条件按空白拆成小写的关键字列表；空条件返回空列表。
fn parse_terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(|term| term.to_lowercase()).collect()
}

/// 命令需要同时包含所有关键字（不区分大小写）。
fn matches_terms(command: &str, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let command = command.to_lowercase();
    terms.iter().all(|term| command.contains(term))
}

/// 在 `text` 中定位所有关键字的大小写不敏感命中区间（原文字节区间）。
///
/// 逐字符构建小写映射，命中区间直接来自原文字符边界，因此多字节字符和
/// 大小写展开（如 `İ` 会展开成两个字符）都不会产生越界的切片。
fn match_ranges(text: &str, terms: &[String]) -> Vec<Range<usize>> {
    let mut lower = String::with_capacity(text.len());
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    for (offset, ch) in text.char_indices() {
        for folded in ch.to_lowercase() {
            lower.push(folded);
            starts.push(offset);
            ends.push(offset + ch.len_utf8());
        }
    }

    let haystack: Vec<char> = lower.chars().collect();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for term in terms {
        let needle: Vec<char> = term.chars().collect();
        if needle.is_empty() || needle.len() > haystack.len() {
            continue;
        }
        let mut index = 0;
        while index + needle.len() <= haystack.len() {
            if haystack[index..index + needle.len()] == needle[..] {
                ranges.push(starts[index]..ends[index + needle.len() - 1]);
                index += needle.len();
            } else {
                index += 1;
            }
        }
    }

    ranges.sort_by_key(|range| (range.start, range.end));
    // 合并重叠/相邻区间，避免同一片段被重复高亮。
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        let overlaps_with_last = merged.last().is_some_and(|last| range.start <= last.end);
        if !overlaps_with_last {
            merged.push(range);
        } else if let Some(last) = merged.last_mut() {
            last.end = last.end.max(range.end);
        }
    }
    merged
}

/// 取字符串末尾 `max_chars` 个字符，用于在有限宽度内展示检索条件。
fn tail_chars(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    text.chars().skip(char_count - max_chars).collect()
}

/// 把一行命令拆成带高亮的 spans，命中的检索词用黄色加粗显示。
fn build_line(display: &str, terms: &[String], base_style: Style, is_selected: bool) -> Line<'static> {
    let marker = if is_selected { " > " } else { "   " };
    let mut spans = vec![Span::styled(marker.to_string(), base_style)];

    let ranges = match_ranges(display, terms);
    if ranges.is_empty() {
        spans.push(Span::styled(display.to_string(), base_style));
        return Line::from(spans);
    }

    let highlight_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let mut cursor = 0;
    for range in ranges {
        if range.start > cursor {
            spans.push(Span::styled(display[cursor..range.start].to_string(), base_style));
        }
        spans.push(Span::styled(display[range.clone()].to_string(), highlight_style));
        cursor = range.end;
    }
    if cursor < display.len() {
        spans.push(Span::styled(display[cursor..].to_string(), base_style));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn entry(command: &str) -> HistoryEntry {
        HistoryEntry {
            command: command.to_string(),
            cli: None,
        }
    }

    fn make_selector(commands: &[&str]) -> HistorySelector {
        HistorySelector::new(commands.iter().map(|command| entry(command)).collect())
    }

    fn press(selector: &mut HistorySelector, code: KeyCode) {
        selector
            .handle_keypress(KeyEvent::new(code, KeyModifiers::NONE))
            .expect("key handling should not fail");
    }

    fn press_ctrl(selector: &mut HistorySelector, code: KeyCode) {
        selector
            .handle_keypress(KeyEvent::new(code, KeyModifiers::CONTROL))
            .expect("key handling should not fail");
    }

    fn type_text(selector: &mut HistorySelector, text: &str) {
        for ch in text.chars() {
            press(selector, KeyCode::Char(ch));
        }
    }

    fn visible_commands(selector: &HistorySelector) -> Vec<&str> {
        selector
            .filtered
            .iter()
            .map(|index| selector.entries[*index].command.as_str())
            .collect()
    }

    fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let width = usize::from(buffer.area.width);
        let mut text = String::new();
        for (index, cell) in buffer.content().iter().enumerate() {
            text.push_str(cell.symbol());
            if width > 0 && (index + 1) % width == 0 {
                text.push('\n');
            }
        }
        text
    }

    #[test]
    fn typing_filters_history_case_insensitively() {
        let mut selector = make_selector(&["rcurl https://Example.com/a", "rcurl https://rust-lang.org"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "EXAMPLE");

        assert_eq!(selector.query, "EXAMPLE");
        assert_eq!(visible_commands(&selector), vec!["rcurl https://Example.com/a"]);
        assert_eq!(selector.selected, 0);
    }

    #[test]
    fn multi_term_query_requires_every_term() {
        let mut selector = make_selector(&[
            "rcurl -X POST https://a.com -d '{\"k\":1}'",
            "rcurl -X GET https://a.com",
            "rcurl https://b.com",
        ]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "post a.com");

        assert_eq!(
            visible_commands(&selector),
            vec!["rcurl -X POST https://a.com -d '{\"k\":1}'"]
        );
    }

    #[test]
    fn unicode_query_filters_commands() {
        let mut selector = make_selector(&["rcurl https://例子.中国/中文", "rcurl https://example.com"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "中文");

        assert_eq!(selector.query, "中文");
        assert_eq!(visible_commands(&selector), vec!["rcurl https://例子.中国/中文"]);
    }

    #[test]
    fn filtering_resets_selection_to_the_first_match() {
        let mut selector = make_selector(&["rcurl https://a.example.com", "rcurl https://b.example.com"]);

        press(&mut selector, KeyCode::Down);
        assert_eq!(selector.selected, 1);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "b.example");

        assert_eq!(selector.selected, 0);
        assert_eq!(selector.filtered.len(), 1);
        assert_eq!(selector.list_state.selected(), Some(0));
    }

    #[test]
    fn q_is_typed_while_searching() {
        let mut selector = make_selector(&["rcurl https://queue.example.com", "rcurl https://example.com"]);

        press(&mut selector, KeyCode::Char('/'));
        press(&mut selector, KeyCode::Char('q'));

        assert!(!selector.should_exit);
        assert_eq!(selector.query, "q");
        assert_eq!(visible_commands(&selector), vec!["rcurl https://queue.example.com"]);
    }

    #[test]
    fn slash_is_typed_into_the_query_while_searching() {
        let mut selector = make_selector(&["rcurl https://example.com", "rcurl ftp://example.com"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "https://");

        assert_eq!(selector.query, "https://");
        assert_eq!(visible_commands(&selector), vec!["rcurl https://example.com"]);
    }

    #[test]
    fn escape_clears_the_query_before_quitting() {
        let mut selector = make_selector(&["rcurl https://example.com", "rcurl https://rust-lang.org"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "example");
        press(&mut selector, KeyCode::Esc);

        assert!(!selector.should_exit);
        assert_eq!(selector.focus, Focus::List);
        assert!(selector.query.is_empty());
        assert_eq!(selector.filtered.len(), 2);
        assert_eq!(selector.outcome(), None);

        // 列表态再按 Esc 才取消退出。
        press(&mut selector, KeyCode::Esc);
        assert!(selector.should_exit);
        assert_eq!(selector.outcome(), None);
    }

    #[test]
    fn escape_with_an_empty_query_just_returns_to_the_list() {
        let mut selector = make_selector(&["rcurl https://example.com"]);

        press(&mut selector, KeyCode::Char('/'));
        press(&mut selector, KeyCode::Esc);

        assert!(!selector.should_exit);
        assert_eq!(selector.focus, Focus::List);
        assert_eq!(selector.filtered.len(), 1);
    }

    #[test]
    fn enter_confirms_the_selected_command() {
        let mut selector = make_selector(&["rcurl https://example.com", "rcurl https://rust-lang.org"]);

        press(&mut selector, KeyCode::Down);
        press(&mut selector, KeyCode::Enter);

        assert!(selector.should_exit);
        assert_eq!(selector.outcome().as_deref(), Some("rcurl https://rust-lang.org"));
    }

    #[test]
    fn enter_while_searching_runs_the_highlighted_match() {
        let mut selector = make_selector(&["rcurl https://example.com", "rcurl https://rust-lang.org"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "rust");
        press(&mut selector, KeyCode::Enter);

        assert!(selector.should_exit);
        assert_eq!(selector.outcome().as_deref(), Some("rcurl https://rust-lang.org"));
    }

    #[test]
    fn enter_without_matches_does_not_confirm() {
        let mut selector = make_selector(&["rcurl https://example.com"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "zzz");
        press(&mut selector, KeyCode::Enter);

        assert!(selector.filtered.is_empty());
        assert!(!selector.should_exit);
        assert_eq!(selector.outcome(), None);
        assert_eq!(selector.list_state.selected(), None);
    }

    #[test]
    fn navigation_is_clamped_to_the_filtered_list() {
        let mut selector = make_selector(&["rcurl a", "rcurl b"]);

        press(&mut selector, KeyCode::Down);
        press(&mut selector, KeyCode::Down);
        assert_eq!(selector.selected, 1);

        press(&mut selector, KeyCode::Up);
        press(&mut selector, KeyCode::Up);
        assert_eq!(selector.selected, 0);

        // 0 命中时移动选中项不应 panic。
        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "zzz");
        press(&mut selector, KeyCode::Down);
        assert_eq!(selector.selected, 0);
        assert_eq!(selector.list_state.selected(), None);
    }

    #[test]
    fn home_and_end_follow_the_filtered_results() {
        let mut selector = make_selector(&[
            "rcurl https://a.example.com",
            "rcurl https://b.example.com",
            "rcurl https://c.example.com",
        ]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "example");
        press(&mut selector, KeyCode::End);
        assert_eq!(selector.selected, 2);

        press(&mut selector, KeyCode::Home);
        assert_eq!(selector.selected, 0);
    }

    #[test]
    fn ctrl_u_clears_the_query() {
        let mut selector = make_selector(&["rcurl https://example.com", "rcurl https://rust-lang.org"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "example");
        assert_eq!(selector.filtered.len(), 1);

        press_ctrl(&mut selector, KeyCode::Char('u'));

        assert!(selector.query.is_empty());
        assert_eq!(selector.filtered.len(), 2);
    }

    #[test]
    fn backspace_removes_a_whole_character() {
        let mut selector = make_selector(&["rcurl https://例子.中国/中文"]);

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "中文");
        press(&mut selector, KeyCode::Backspace);

        assert_eq!(selector.query, "中");
        assert_eq!(selector.filtered.len(), 1);
    }

    #[test]
    fn ctrl_c_cancels_from_both_focus_states() {
        let mut selector = make_selector(&["rcurl https://example.com"]);

        press(&mut selector, KeyCode::Char('/'));
        press_ctrl(&mut selector, KeyCode::Char('c'));
        assert!(selector.should_exit);
        assert_eq!(selector.outcome(), None);

        let mut selector = make_selector(&["rcurl https://example.com"]);
        press_ctrl(&mut selector, KeyCode::Char('c'));
        assert!(selector.should_exit);
        assert_eq!(selector.outcome(), None);
    }

    #[test]
    fn q_still_quits_from_the_list() {
        let mut selector = make_selector(&["rcurl https://example.com"]);

        press(&mut selector, KeyCode::Char('q'));

        assert!(selector.should_exit);
        assert_eq!(selector.outcome(), None);
    }

    #[test]
    fn match_ranges_are_case_insensitive_and_merged() {
        let terms = vec!["example".to_string()];
        let ranges = match_ranges("Example example EXAMPLE", &terms);

        assert_eq!(ranges, vec![0..7, 8..15, 16..23]);
    }

    #[test]
    fn match_ranges_handle_multibyte_text() {
        let terms = vec!["中国".to_string()];
        let ranges = match_ranges("https://例子.中国/路径", &terms);

        assert_eq!(ranges, vec![15..21]);

        // `İ` 的小写形式是两个字符（i + 组合符），不应产生越界切片。
        let ranges = match_ranges("İ", &["i".to_string()]);
        assert_eq!(ranges, vec![0..2]);
    }

    #[test]
    fn search_box_renders_inside_a_small_terminal() {
        let mut selector = make_selector(&["rcurl https://example.com", "rcurl https://rust-lang.org"]);
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("test terminal");

        terminal
            .draw(|frame| selector.ui(frame))
            .expect("selector should render");
        let text = rendered_text(&terminal);
        assert!(text.contains("Command History"));
        assert!(text.contains("Press / to search"));
        assert!(text.contains("Enter run"));

        press(&mut selector, KeyCode::Char('/'));
        type_text(&mut selector, "exa");
        terminal
            .draw(|frame| selector.ui(frame))
            .expect("filtered selector should render");
        let text = rendered_text(&terminal);
        assert!(text.contains("Search: exa"));
        assert!(text.contains("[1/1]"));

        // 极小的终端也不能 panic。
        let mut tiny = Terminal::new(TestBackend::new(30, 8)).expect("test terminal");
        tiny.draw(|frame| selector.ui(frame))
            .expect("tiny terminal should still render");
    }

    #[test]
    fn tail_chars_keeps_the_end_of_a_multibyte_query() {
        assert_eq!(tail_chars("中文路径", 10), "中文路径");
        assert_eq!(tail_chars("中文路径", 2), "路径");
        assert_eq!(tail_chars("中文路径", 0), "");
    }
}
