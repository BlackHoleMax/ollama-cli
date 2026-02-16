mod ollama;
mod search;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, Tabs},
    DefaultTerminal, Frame,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use ollama::{ChatMessage, OllamaClient};
use search::{ModelSearch, OnlineModel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Chat,
    Models,
    Search,
}

#[derive(Default)]
pub struct AppState {
    current_tab: Tab,
    selected_model: Option<String>,
    models: Vec<ollama::Model>,
    messages: Vec<ChatMessage>,
    input_text: String,
    is_loading: bool,
    search_query: String,
    search_results: Vec<OnlineModel>,
    is_searching: bool,
    model_list_state: ratatui::widgets::ListState,
    search_list_state: ratatui::widgets::ListState,
    chat_scroll: u16,
    auto_scroll: bool,
    status_message: Option<String>,
}

impl AppState {
    fn new() -> Self {
        let mut state = Self::default();
        state.model_list_state.select(Some(0));
        state.search_list_state.select(Some(0));
        state
    }
}

type SharedState = Arc<Mutex<AppState>>;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut terminal = ratatui::init();
    let _ = execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    );

    let state = Arc::new(Mutex::new(AppState::new()));

    let result = run_app(&mut terminal, state);

    disable_raw_mode()?;
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    ratatui::restore();

    result
}

fn run_app(terminal: &mut DefaultTerminal, state: SharedState) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;

    runtime.block_on(async {
        refresh_models(&state).await;
    });

    loop {
        let redraw_interval = {
            let s = state.blocking_lock();
            if s.is_loading {
                50
            } else {
                500
            }
        };

        terminal.draw(|f| {
            let s = state.blocking_lock();
            ui(f, &s);
        })?;

        if event::poll(std::time::Duration::from_millis(redraw_interval))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let state = state.clone();
                    let mut s = state.blocking_lock();

                    match s.current_tab {
                        Tab::Chat => handle_chat_key(&mut s, key.code, &state),
                        Tab::Models => handle_models_input(&mut s, key.code, &state),
                        Tab::Search => handle_search_input(&mut s, key.code, &state),
                    }

                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }

                    if key.code == KeyCode::Tab {
                        s.current_tab = match s.current_tab {
                            Tab::Chat => Tab::Models,
                            Tab::Models => Tab::Search,
                            Tab::Search => Tab::Chat,
                        };
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let tabs = Tabs::new(vec![" Chat ", " Models ", " Search "])
        .select(match state.current_tab {
            Tab::Chat => 0,
            Tab::Models => 1,
            Tab::Search => 2,
        })
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )
        .divider("|");

    frame.render_widget(tabs, chunks[0]);

    match state.current_tab {
        Tab::Chat => render_chat(frame, state, chunks[1]),
        Tab::Models => render_models(frame, state, chunks[1]),
        Tab::Search => render_search(frame, state, chunks[1]),
    }

    let status = state.status_message.clone().unwrap_or_else(|| {
        if state.is_loading {
            " Generating... ".to_string()
        } else {
            let model_info = state
                .selected_model
                .as_ref()
                .map(|m| format!("[{}] ", m))
                .unwrap_or_default();
            match state.current_tab {
                Tab::Chat => format!("{}Enter: send | j/k: scroll | g: top | G: bottom | Tab: switch | q: quit ", model_info),
                Tab::Models => " j/k: select | Enter: use | Tab: switch | q: quit ".to_string(),
                Tab::Search => " j/k: select | Enter: search | Tab: switch | q: quit ".to_string(),
            }
        }
    });

    let footer = Paragraph::new(status)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(""));

    frame.render_widget(footer, chunks[2]);
}

fn render_chat(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    // Split into messages area (flexible) and input area (3 lines)
    let msg_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    if state.messages.is_empty() {
        let welcome = Paragraph::new("")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(welcome, msg_area[0]);
    } else {
        let mut content = String::new();
        for msg in &state.messages {
            let role = match msg.role.as_str() {
                "user" => "You",
                "assistant" => "AI",
                _ => &msg.role,
            };
            content.push_str(&format!("{}: {}\n\n", role, msg.content));
        }

        let total_lines = content.lines().count() as u16;
        let viewport_lines = msg_area[0].height.saturating_sub(2);
        let max_scroll = total_lines.saturating_sub(viewport_lines);
        
        let scroll = if state.auto_scroll {
            max_scroll
        } else {
            state.chat_scroll.min(max_scroll)
        };

        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(" Messages "))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((scroll, 0));

        frame.render_widget(paragraph, msg_area[0]);

        if total_lines > viewport_lines && total_lines > 0 {
            let mut sb_state = ratatui::widgets::ScrollbarState::new(total_lines as usize)
                .position(scroll as usize);
            frame.render_stateful_widget(
                Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight)
                    .thumb_style(Style::default().fg(Color::DarkGray)),
                msg_area[0],
                &mut sb_state,
            );
        }
    }

    let input = Paragraph::new(state.input_text.as_str())
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(" Input "));

    frame.render_widget(input, msg_area[1]);
}

fn render_models(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    let model_items: Vec<ListItem> = state
        .models
        .iter()
        .map(|m| {
            let size_gb = m.size as f64 / 1_073_741_824.0;
            let content = format!("{} ({:.1} GB)", m.name, size_gb);
            ListItem::new(content)
        })
        .collect();

    if model_items.is_empty() {
        let empty =
            Paragraph::new("No models installed. Go to Search tab to find and install models.")
                .style(Style::default().fg(Color::DarkGray))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Installed Models "),
                );
        frame.render_widget(empty, area);
    } else {
        let list = List::new(model_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Installed Models "),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = state.model_list_state.clone();
        frame.render_stateful_widget(list, area, &mut list_state);
    }
}

fn render_search(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let status = if state.is_searching {
        " Searching..."
    } else {
        ""
    };
    let search_input = Paragraph::new(format!("Search: {}{}", state.search_query, status))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search Online Models "),
        );

    frame.render_widget(search_input, chunks[0]);

    let search_items: Vec<ListItem> = state
        .search_results
        .iter()
        .map(|m| ListItem::new(m.name.clone()))
        .collect();

    if search_items.is_empty() {
        let empty = Paragraph::new(
            "Press Enter to load popular models, or type and press Enter to search.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, chunks[1]);
    } else {
        let list = List::new(search_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Search Results "),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = state.search_list_state.clone();
        frame.render_stateful_widget(list, chunks[1], &mut list_state);
    }
}

fn handle_chat_key(state: &mut AppState, key: KeyCode, shared_state: &SharedState) {
    if state.is_loading {
        return;
    }

    match key {
        KeyCode::Char('j') => {
            state.auto_scroll = false;
            let viewport = 5u16;
            state.chat_scroll = state.chat_scroll.saturating_add(viewport);
        }
        KeyCode::Char('k') => {
            state.auto_scroll = false;
            let viewport = 5u16;
            state.chat_scroll = state.chat_scroll.saturating_sub(viewport);
        }
        KeyCode::Char('G') => {
            state.auto_scroll = false;
            state.chat_scroll = u16::MAX;
        }
        KeyCode::Char('g') => {
            state.auto_scroll = false;
            state.chat_scroll = 0;
        }
        KeyCode::Char(c) => {
            state.input_text.push(c);
        }
        KeyCode::Backspace => {
            state.input_text.pop();
        }
        KeyCode::Enter => {
            if !state.input_text.is_empty() && state.selected_model.is_some() {
                let user_input = state.input_text.clone();
                state.input_text.clear();

                state.messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: user_input.clone(),
                });

                state.messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: String::new(),
                });

                let model = state.selected_model.clone().unwrap();
                let messages = state.messages.clone();
                let s_for_callback = shared_state.clone();
                let s_for_join = shared_state.clone();

                state.is_loading = true;
                state.auto_scroll = true;

                let handle = OllamaClient::chat_streaming(model, messages, move |chunk| {
                    let s = s_for_callback.clone();
                    let mut s = s.blocking_lock();
                    if let Some(last) = s.messages.last_mut() {
                        if last.role == "assistant" {
                            last.content = chunk;
                        }
                    }
                });

                std::thread::spawn(move || {
                    let _ = handle.join();
                    let s = s_for_join.clone();
                    let mut s = s.blocking_lock();
                    s.is_loading = false;
                });
            }
        }
        _ => {}
    }
}

fn handle_models_input(state: &mut AppState, key: KeyCode, _shared_state: &SharedState) {
    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(selected) = state.model_list_state.selected() {
                if state.models.is_empty() {
                    return;
                }
                let new_selected = (selected + 1).min(state.models.len() - 1);
                state.model_list_state.select(Some(new_selected));
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(selected) = state.model_list_state.selected() {
                let new_selected = selected.saturating_sub(1);
                state.model_list_state.select(Some(new_selected));
            }
        }
        KeyCode::Char('G') | KeyCode::End => {
            if !state.models.is_empty() {
                state.model_list_state.select(Some(state.models.len() - 1));
            }
        }
        KeyCode::Char('g') => {
            state.model_list_state.select(Some(0));
        }
        KeyCode::Enter => {
            if let Some(selected) = state.model_list_state.selected() {
                if let Some(model) = state.models.get(selected) {
                    state.selected_model = Some(model.name.clone());
                    state.current_tab = Tab::Chat;
                }
            }
        }
        _ => {}
    }
}

fn handle_search_input(state: &mut AppState, key: KeyCode, shared_state: &SharedState) {
    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(selected) = state.search_list_state.selected() {
                if state.search_results.is_empty() {
                    return;
                }
                let new_selected = (selected + 1).min(state.search_results.len() - 1);
                state.search_list_state.select(Some(new_selected));
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(selected) = state.search_list_state.selected() {
                let new_selected = selected.saturating_sub(1);
                state.search_list_state.select(Some(new_selected));
            }
        }
        KeyCode::Char('G') | KeyCode::End => {
            if !state.search_results.is_empty() {
                state
                    .search_list_state
                    .select(Some(state.search_results.len() - 1));
            }
        }
        KeyCode::Char('g') => {
            state.search_list_state.select(Some(0));
        }
        KeyCode::Char(c) => {
            state.search_query.push(c);
        }
        KeyCode::Backspace => {
            state.search_query.pop();
        }
        KeyCode::Enter => {
            if !state.is_searching {
                let query = state.search_query.clone();
                let s = shared_state.clone();

                state.is_searching = true;

                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    let searcher = ModelSearch::new();

                    let results = if query.is_empty() {
                        searcher.get_popular_models().unwrap_or_default()
                    } else {
                        searcher.search_online(&query).unwrap_or_default()
                    };

                    rt.block_on(async {
                        let mut s = s.lock().await;
                        s.search_results = results;
                        s.is_searching = false;
                    });
                });
            }
        }
        _ => {}
    }
}

async fn refresh_models(state: &SharedState) {
    let client = OllamaClient::new(None);
    match client.list_models().await {
        Ok(response) => {
            let mut s = state.lock().await;
            s.models = response.models;
        }
        Err(e) => {
            let mut s = state.lock().await;
            s.status_message = Some(format!(
                "Failed to connect: {}. Make sure Ollama is running.",
                e
            ));
        }
    }
}
