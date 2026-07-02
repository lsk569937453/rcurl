use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, ListState},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;

use crate::history::types::HistoryEntry;

pub struct HistorySelector {
    entries: Vec<HistoryEntry>,
    selected: usize,
    should_exit: bool,
    list_state: ListState,
}

impl HistorySelector {
    pub fn new(entries: Vec<HistoryEntry>) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            entries,
            selected: 0,
            should_exit: false,
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

        if self.entries.is_empty() {
            Ok(None)
        } else {
            Ok(Some(self.entries[self.selected].command.clone()))
        }
    }

    fn handle_keypress(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_exit = true;
                return Ok(());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.entries.is_empty() {
                    self.selected = (self.selected + 1).min(self.entries.len() - 1);
                    self.list_state.select(Some(self.selected));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            KeyCode::Enter => {
                self.should_exit = true;
            }
            KeyCode::Home => {
                self.selected = 0;
                self.list_state.select(Some(self.selected));
            }
            KeyCode::End => {
                if !self.entries.is_empty() {
                    self.selected = self.entries.len() - 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)].as_ref())
            .split(f.area());

        // Header
        let header = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Command History", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Use ", Style::default().fg(Color::Gray)),
                Span::styled("↑/j", Style::default().fg(Color::Yellow)),
                Span::styled(" and ", Style::default().fg(Color::Gray)),
                Span::styled("↓/k", Style::default().fg(Color::Yellow)),
                Span::styled(" to navigate, ", Style::default().fg(Color::Gray)),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::styled(" to select, ", Style::default().fg(Color::Gray)),
                Span::styled("q/Esc", Style::default().fg(Color::Yellow)),
                Span::styled(" to quit", Style::default().fg(Color::Gray)),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(header, chunks[0]);

        // History list
        if self.entries.is_empty() {
            let empty_msg = Paragraph::new("No request history found. Run a command first to create history.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            f.render_widget(empty_msg, chunks[1]);
        } else {
            let items: Vec<ListItem> = self.entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let display_cmd = entry.display_terminal_command(100);
                    let is_selected = i == self.selected;

                    let content = if is_selected {
                        vec![
                            Line::from(Span::styled(format!(" > {}", display_cmd), Style::default().fg(Color::Green)))
                        ]
                    } else {
                        vec![
                            Line::from(Span::styled(format!("   {}", display_cmd), Style::default().fg(Color::White)))
                        ]
                    };

                    ListItem::new(content)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD));

            f.render_stateful_widget(list, chunks[1], &mut self.list_state);
        }

        // Footer
        let footer_text = if self.entries.is_empty() {
            "Press q/Esc to quit"
        } else {
            &format!("[{}/{}] Selected: Enter | Quit: q/Esc", self.selected + 1, self.entries.len())
        };

        let footer = Paragraph::new(Line::from(vec![
            Span::styled(footer_text, Style::default().fg(Color::Gray)),
        ]))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(footer, chunks[2]);
    }
}
