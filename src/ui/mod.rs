use colored::Colorize;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io::{self, stdout};
use std::time::Duration;

use crate::backend::{InstallResult, Package};

/// Safely truncate a string to a maximum number of characters (not bytes)
fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

pub struct FuzzyFinder {
    packages: Vec<Package>,
    filtered: Vec<usize>,
    query: String,
    cursor: usize,
    selected: Vec<usize>,
    list_state: ListState,
}

impl FuzzyFinder {
    pub fn new(packages: Vec<Package>) -> Self {
        let filtered: Vec<usize> = (0..packages.len()).collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            packages,
            filtered,
            query: String::new(),
            cursor: 0,
            selected: Vec::new(),
            list_state,
        }
    }

    fn filter(&mut self) {
        let query_lower = self.query.to_lowercase();

        if query_lower.is_empty() {
            self.filtered = (0..self.packages.len()).collect();
        } else {
            self.filtered = self
                .packages
                .iter()
                .enumerate()
                .filter(|(_, pkg)| {
                    let name_match = pkg.name.to_lowercase().contains(&query_lower);
                    let desc_match = pkg
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false);
                    name_match || desc_match
                })
                .map(|(i, _)| i)
                .collect();
        }

        // Reset cursor if out of bounds
        if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
        }
        self.list_state.select(if self.filtered.is_empty() {
            None
        } else {
            Some(self.cursor)
        });
    }

    fn toggle_selection(&mut self) {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            if let Some(pos) = self.selected.iter().position(|&x| x == idx) {
                self.selected.remove(pos);
            } else {
                self.selected.push(idx);
            }
        }
    }

    fn move_up(&mut self) {
        if !self.filtered.is_empty() {
            self.cursor = self.cursor.saturating_sub(1);
            self.list_state.select(Some(self.cursor));
        }
    }

    fn move_down(&mut self) {
        if !self.filtered.is_empty() && self.cursor < self.filtered.len() - 1 {
            self.cursor += 1;
            self.list_state.select(Some(self.cursor));
        }
    }

    pub fn get_selected_packages(&self) -> Vec<Package> {
        self.selected
            .iter()
            .filter_map(|&idx| self.packages.get(idx).cloned())
            .collect()
    }

    pub fn run(mut self) -> io::Result<Vec<Package>> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

        result
    }

    fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<Vec<Package>> {
        loop {
            terminal.draw(|f| self.render(f))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc => {
                            return Ok(vec![]);
                        }
                        KeyCode::Enter => {
                            // If nothing selected, select current item
                            if self.selected.is_empty() {
                                if let Some(&idx) = self.filtered.get(self.cursor) {
                                    self.selected.push(idx);
                                }
                            }
                            return Ok(self.get_selected_packages());
                        }
                        KeyCode::Tab | KeyCode::Char(' ') => {
                            self.toggle_selection();
                            self.move_down();
                        }
                        KeyCode::Up | KeyCode::BackTab => {
                            self.move_up();
                        }
                        KeyCode::Down => {
                            self.move_down();
                        }
                        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.move_up();
                        }
                        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.move_down();
                        }
                        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Select all filtered
                            for &idx in &self.filtered {
                                if !self.selected.contains(&idx) {
                                    self.selected.push(idx);
                                }
                            }
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(vec![]);
                        }
                        KeyCode::Backspace => {
                            self.query.pop();
                            self.filter();
                        }
                        KeyCode::Char(c) => {
                            self.query.push(c);
                            self.filter();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Min(5),    // List
                Constraint::Length(3), // Help
            ])
            .split(f.area());

        // Search input
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta))
            .title(" Search (type to filter) ");

        let search_text = format!(" > {}_", self.query);
        let search = Paragraph::new(search_text)
            .style(Style::default().fg(Color::White))
            .block(search_block);
        f.render_widget(search, chunks[0]);

        // Package list
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(display_idx, &pkg_idx)| {
                let pkg = &self.packages[pkg_idx];
                let is_selected = self.selected.contains(&pkg_idx);

                let checkbox = if is_selected { "[x]" } else { "[ ]" };
                let ood = if pkg.extra.out_of_date.is_some() {
                    " !"
                } else {
                    ""
                };

                // Format popularity/votes info
                let pop_info = if let Some(votes) = pkg.extra.aur_votes {
                    format!("(+{})", votes)
                } else if pkg.popularity > 0.0 {
                    format!("({:.1})", pkg.popularity)
                } else {
                    String::new()
                };

                let desc = pkg.description.as_deref().unwrap_or("No description");
                let desc_short = truncate_str(desc, 50);

                let line = format!(
                    "{} {} {} {}{}  {}",
                    checkbox, pkg.name, pkg.version, pop_info, ood, desc_short
                );

                let style = if display_idx == self.cursor {
                    Style::default()
                        .bg(Color::Rgb(60, 60, 80))
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default().fg(Color::Green)
                } else if pkg.extra.out_of_date.is_some() {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };

                ListItem::new(line).style(style)
            })
            .collect();

        let list_title = format!(
            " Packages ({}/{}) - {} selected ",
            self.filtered.len(),
            self.packages.len(),
            self.selected.len()
        );

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(list_title),
            )
            .highlight_style(Style::default().bg(Color::Rgb(60, 60, 80)));

        f.render_stateful_widget(list, chunks[1], &mut self.list_state);

        // Help bar
        let help_text = " [Space/Tab] Select  [Enter] Confirm  [Esc] Cancel  [Ctrl+A] Select All  [Up/Down] Navigate ";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        f.render_widget(help, chunks[2]);
    }
}

pub struct LiveSearcher {
    query: String,
    results: Vec<Package>,
    suggestions: Vec<Package>,
    cursor: usize,
    selected: Vec<usize>,
    list_state: ListState,
    loading: bool,
    last_query: String,
    pm_name: String,
}

impl LiveSearcher {
    pub fn new(pm_name: &str) -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            suggestions: Vec::new(),
            cursor: 0,
            selected: Vec::new(),
            list_state: ListState::default(),
            loading: false,
            last_query: String::new(),
            pm_name: pm_name.to_string(),
        }
    }

    pub fn set_results(&mut self, results: Vec<Package>) {
        self.results = results;
        self.suggestions.clear(); // Clear suggestions when real results arrive
        self.cursor = 0;
        self.selected.clear();
        self.list_state.select(if self.results.is_empty() {
            None
        } else {
            Some(0)
        });
        self.loading = false;
    }

    pub fn set_suggestions(&mut self, suggestions: Vec<Package>) {
        self.suggestions = suggestions;
        self.cursor = 0;
        self.selected.clear();
        self.list_state.select(if self.suggestions.is_empty() {
            None
        } else {
            Some(0)
        });
    }

    pub fn clear_suggestions(&mut self) {
        self.suggestions.clear();
        if self.results.is_empty() {
            self.list_state.select(None);
        }
    }

    pub fn has_suggestions(&self) -> bool {
        !self.suggestions.is_empty()
    }

    pub fn has_results(&self) -> bool {
        !self.results.is_empty()
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn get_query(&self) -> &str {
        &self.query
    }

    pub fn needs_search(&self) -> bool {
        self.query.len() >= 2 && self.query != self.last_query
    }

    pub fn is_query_too_short(&self) -> bool {
        !self.query.is_empty() && self.query.len() < 2
    }

    pub fn mark_searched(&mut self) {
        self.last_query = self.query.clone();
    }

    fn toggle_selection(&mut self) {
        let items = if !self.results.is_empty() {
            self.results.len()
        } else {
            self.suggestions.len()
        };
        if self.cursor < items {
            if let Some(pos) = self.selected.iter().position(|&x| x == self.cursor) {
                self.selected.remove(pos);
            } else {
                self.selected.push(self.cursor);
            }
        }
    }

    fn move_up(&mut self) {
        let items = if !self.results.is_empty() {
            self.results.len()
        } else {
            self.suggestions.len()
        };
        if items > 0 {
            self.cursor = self.cursor.saturating_sub(1);
            self.list_state.select(Some(self.cursor));
        }
    }

    fn move_down(&mut self) {
        let items = if !self.results.is_empty() {
            self.results.len()
        } else {
            self.suggestions.len()
        };
        if items > 0 && self.cursor < items - 1 {
            self.cursor += 1;
            self.list_state.select(Some(self.cursor));
        }
    }

    pub fn get_selected_packages(&self) -> Vec<Package> {
        let packages = if !self.results.is_empty() {
            &self.results
        } else {
            &self.suggestions
        };
        self.selected
            .iter()
            .filter_map(|&idx| packages.get(idx).cloned())
            .collect()
    }

    pub fn get_current_package(&self) -> Option<Package> {
        if !self.results.is_empty() {
            self.results.get(self.cursor).cloned()
        } else {
            self.suggestions.get(self.cursor).cloned()
        }
    }

    pub fn handle_key(&mut self, key: event::KeyEvent) -> Option<SearchAction> {
        match key.code {
            KeyCode::Esc => Some(SearchAction::Quit),
            KeyCode::Enter => {
                if self.selected.is_empty() {
                    if let Some(pkg) = self.get_current_package() {
                        return Some(SearchAction::Install(vec![pkg]));
                    }
                }
                let selected = self.get_selected_packages();
                if !selected.is_empty() {
                    Some(SearchAction::Install(selected))
                } else {
                    None
                }
            }
            KeyCode::Tab | KeyCode::Char(' ')
                if !self.results.is_empty() || !self.suggestions.is_empty() =>
            {
                self.toggle_selection();
                self.move_down();
                None
            }
            KeyCode::Up => {
                self.move_up();
                None
            }
            KeyCode::Down => {
                self.move_down();
                None
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_up();
                None
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_down();
                None
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(SearchAction::Quit)
            }
            KeyCode::Backspace => {
                self.query.pop();
                None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                None
            }
            _ => None,
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Min(5),    // List
                Constraint::Length(3), // Help
            ])
            .split(f.area());

        // Search input with loading indicator
        let loading_indicator = if self.loading { " ..." } else { "" };
        let search_text = format!(" > {}{}_", self.query, loading_indicator);

        let title = if !self.suggestions.is_empty() && self.results.is_empty() {
            format!(" zap ⚡ {} (suggestions) ", self.pm_name)
        } else {
            format!(" zap ⚡ {} (min 2 chars) ", self.pm_name)
        };
        let search = Paragraph::new(search_text)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta))
                    .title(title),
            );
        f.render_widget(search, chunks[0]);

        // Package list - show suggestions if no results, otherwise show results
        let empty_vec = Vec::<Package>::new();
        let display_packages = if !self.results.is_empty() {
            &self.results
        } else if !self.suggestions.is_empty() {
            &self.suggestions
        } else {
            &empty_vec
        };

        let items: Vec<ListItem> = if self.is_query_too_short() && display_packages.is_empty() {
            vec![ListItem::new("  Type at least 2 characters to search...")
                .style(Style::default().fg(Color::DarkGray))]
        } else if display_packages.is_empty() && !self.query.is_empty() && !self.loading {
            vec![ListItem::new("  No packages found").style(Style::default().fg(Color::DarkGray))]
        } else {
            display_packages
                .iter()
                .enumerate()
                .map(|(idx, pkg)| {
                    let is_selected = self.selected.contains(&idx);

                    let checkbox = if is_selected { "[x]" } else { "[ ]" };
                    let ood = if pkg.extra.out_of_date.is_some() {
                        " [!]"
                    } else {
                        ""
                    };
                    let installed = if pkg.installed { " [installed]" } else { "" };

                    // Format popularity/votes info
                    let pop_info = if let Some(votes) = pkg.extra.aur_votes {
                        format!("(+{})", votes)
                    } else if pkg.popularity > 0.0 {
                        format!("({:.1})", pkg.popularity)
                    } else {
                        String::new()
                    };

                    let desc = pkg.description.as_deref().unwrap_or("No description");
                    let desc_short = truncate_str(desc, 40);

                    let line = format!(
                        "{} {} {} {}{}{}  {}",
                        checkbox, pkg.name, pkg.version, pop_info, ood, installed, desc_short
                    );

                    let style = if idx == self.cursor {
                        Style::default()
                            .bg(Color::Rgb(80, 60, 120))
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else if is_selected {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    } else if pkg.installed {
                        Style::default().fg(Color::Blue)
                    } else if pkg.extra.out_of_date.is_some() {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Gray)
                    };

                    ListItem::new(line).style(style)
                })
                .collect()
        };

        let display_count = if !self.results.is_empty() {
            self.results.len()
        } else {
            self.suggestions.len()
        };

        let list_title = if !self.results.is_empty() {
            format!(
                " Results: {} | Selected: {} ",
                display_count,
                self.selected.len()
            )
        } else if !self.suggestions.is_empty() {
            format!(
                " Suggestions: {} | Selected: {} ",
                display_count,
                self.selected.len()
            )
        } else {
            format!(" Results: 0 | Selected: {} ", self.selected.len())
        };

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(list_title),
        );

        f.render_stateful_widget(list, chunks[1], &mut self.list_state);

        // Help bar
        let help_text = " Space:Select | Enter:Install | Esc:Quit | Up/Down:Navigate ";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        f.render_widget(help, chunks[2]);
    }
}

pub enum SearchAction {
    Quit,
    Install(Vec<Package>),
}

// Console output helpers
pub fn print_package_details(pkg: &Package) {
    println!();
    println!("Package: {}", pkg.name.cyan().bold());
    println!("Version: {}", pkg.version.green());

    if let Some(desc) = &pkg.description {
        println!("Description: {}", desc);
    }

    if let Some(url) = &pkg.url {
        println!("URL: {}", url.blue());
    }

    if let Some(maintainer) = &pkg.maintainer {
        println!("Maintainer: {}", maintainer);
    }

    if let Some(votes) = pkg.extra.aur_votes {
        println!("Votes: {}", votes.to_string().yellow());
    }

    if pkg.popularity > 0.0 {
        println!("Popularity: {:.2}", pkg.popularity);
    }

    if !pkg.extra.depends.is_empty() {
        println!(
            "Dependencies: {}",
            pkg.extra.depends.join(", ").bright_black()
        );
    }

    if pkg.extra.out_of_date.is_some() {
        println!(
            "{}",
            "WARNING: This package is flagged as out of date!"
                .red()
                .bold()
        );
    }

    if pkg.installed {
        println!("Status: {}", "Installed".green().bold());
    }

    if !pkg.extra.license.is_empty() {
        println!("License: {}", pkg.extra.license.join(", "));
    }

    println!();
}

pub fn print_search_results(packages: &[Package], pm_name: &str) {
    println!();
    println!(
        "{} {} packages found ({})",
        "-->".green(),
        packages.len().to_string().cyan().bold(),
        pm_name.bright_black()
    );
    println!();

    for (i, pkg) in packages.iter().enumerate() {
        let ood_marker = if pkg.extra.out_of_date.is_some() {
            format!(" {}", "[OOD]".red())
        } else {
            String::new()
        };

        let installed_marker = if pkg.installed {
            format!(" {}", "[installed]".blue())
        } else {
            String::new()
        };

        // Format popularity/votes info
        let pop_info = if let Some(votes) = pkg.extra.aur_votes {
            format!("(+{})", votes).yellow().to_string()
        } else {
            String::new()
        };

        println!(
            "{:>3}. {} {} {}{}{}",
            (i + 1).to_string().bright_black(),
            pkg.name.cyan().bold(),
            pkg.version.green(),
            pop_info,
            ood_marker,
            installed_marker
        );

        if let Some(desc) = &pkg.description {
            let desc_lines: Vec<&str> = desc.lines().collect();
            if let Some(first_line) = desc_lines.first() {
                let truncated = truncate_str(first_line, 70);
                println!("     {}", truncated.bright_black());
            }
        }
    }
    println!();
}

pub fn print_install_summary(results: &[InstallResult]) {
    println!();
    println!("{}", "=".repeat(60).bright_black());
    println!("Installation Summary");
    println!("{}", "=".repeat(60).bright_black());

    let mut success = 0;
    let mut failed = 0;

    for result in results {
        if result.success {
            println!(
                "  {} {} installed successfully",
                "[OK]".green(),
                result.package.cyan()
            );
            success += 1;
        } else {
            let msg = result.message.as_deref().unwrap_or("Unknown error");
            println!(
                "  {} {} failed: {}",
                "[ERR]".red(),
                result.package.cyan(),
                msg
            );
            failed += 1;
        }
    }

    println!("{}", "=".repeat(60).bright_black());
    println!(
        "  {} {} succeeded, {} failed",
        "-->".green(),
        success.to_string().green().bold(),
        failed.to_string().red().bold()
    );
    println!();
}

pub fn print_error(msg: &str) {
    eprintln!("{} {}", "[ERROR]".red().bold(), msg);
}

pub fn print_success(msg: &str) {
    println!("{} {}", "[OK]".green(), msg);
}

pub fn print_info(msg: &str) {
    println!("{} {}", "-->".blue(), msg);
}

pub fn print_warning(msg: &str) {
    println!("{} {}", "[WARN]".yellow(), msg);
}
