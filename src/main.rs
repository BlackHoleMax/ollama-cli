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
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
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
    chat_scroll_state: ratatui::widgets::ListState,
    input_mode: bool,
    status_message: Option<String>,
}

impl AppState {
    fn new() -> Self {
        let mut state = Self::default();
        state.model_list_state.select(Some(0));
        state.search_list_state.select(Some(0));
        state.chat_scroll_state.select(Some(0));
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
        terminal.draw(|f| {
            let s = state.blocking_lock();
            ui(f, &s);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                let state = state.clone();
                let mut s = state.blocking_lock();

                match s.current_tab {
                    Tab::Chat => handle_chat_input(&mut s, key.code, &state),
                    Tab::Models => handle_models_input(&mut s, key.code, &state),
                    Tab::Search => handle_search_input(&mut s, key.code, &state),
                }

                if key.code == KeyCode::Esc {
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
            " Loading... ".to_string()
        } else {
            match state.current_tab {
                Tab::Chat => {
                    if state.input_mode {
                        " INSERT: typing... | Esc: exit insert | Enter: send ".to_string()
                    } else {
                        " NORMAL: j/k: scroll | g: top | G: bottom | i/a/Enter: input | d: del msg | Tab: switch | Esc: quit ".to_string()
                    }
                }
                Tab::Models => " j/k: select | Enter: use | d: delete | r: refresh | Tab: switch | Esc: quit ".to_string(),
                Tab::Search => " j/k: select | Enter: search | Tab: switch | Esc: quit ".to_string(),
            }
        }
    });

    let footer = Paragraph::new(status)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(""));

    frame.render_widget(footer, chunks[2]);
}

fn render_chat(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let model_name = state
        .selected_model
        .as_deref()
        .unwrap_or("No model selected");
    let header = Paragraph::new(format!("Model: {}", model_name))
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::NONE).title(" Chat "));

    frame.render_widget(header, chunks[0]);

    if state.messages.is_empty() {
        let welcome = Paragraph::new("Welcome! Select a model from Models tab to start chatting.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(welcome, chunks[1]);
    } else {
        let items: Vec<ListItem> = state
            .messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let role = match msg.role.as_str() {
                    "user" => "You",
                    "assistant" => "AI",
                    _ => &msg.role,
                };
                let style = if msg.role == "user" {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Green)
                };
                let content = format!("{}: {}", role, msg.content);
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Messages "))
            .highlight_style(Style::default())
            .highlight_symbol("")
            .scroll_padding(1);

        let mut scroll_state = state.chat_scroll_state.clone();
        frame.render_stateful_widget(list, chunks[1], &mut scroll_state);
    }

    let input_mode_title = if state.input_mode { " INSERT " } else { " NORMAL " };
    let input = Paragraph::new(state.input_text.as_str())
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(input_mode_title));

    frame.render_widget(input, chunks[2]);
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

fn handle_chat_input(state: &mut AppState, key: KeyCode, shared_state: &SharedState) {
    if state.input_mode {
        match key {
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

                    let model = state.selected_model.clone().unwrap();
                    let messages = state.messages.clone();
                    let s = shared_state.clone();

                    state.is_loading = true;
                    state.input_mode = false;

                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            let client = OllamaClient::new(None);
                            match client.chat(&model, messages).await {
                                Ok(response) => {
                                    let mut s = s.lock().await;
                                    s.messages.push(response.message);
                                    s.is_loading = false;
                                    s.input_mode = true;
                                }
                                Err(e) => {
                                    let mut s = s.lock().await;
                                    s.status_message = Some(format!("Error: {}", e));
                                    s.is_loading = false;
                                    s.input_mode = true;
                                }
                            }
                        });
                    });
                }
            }
            KeyCode::Esc => {
                state.input_mode = false;
            }
            _ => {}
        }
    } else {
        match key {
            KeyCode::Char('i') | KeyCode::Char('a') | KeyCode::Enter => {
                if state.selected_model.is_some() {
                    state.input_mode = true;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(selected) = state.chat_scroll_state.selected() {
                    if state.messages.is_empty() {
                        return;
                    }
                    let new_selected = (selected + 1).min(state.messages.len() - 1);
                    state.chat_scroll_state.select(Some(new_selected));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(selected) = state.chat_scroll_state.selected() {
                    let new_selected = selected.saturating_sub(1);
                    state.chat_scroll_state.select(Some(new_selected));
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if !state.messages.is_empty() {
                    state.chat_scroll_state.select(Some(state.messages.len() - 1));
                }
            }
            KeyCode::Char('g') => {
                state.chat_scroll_state.select(Some(0));
            }
            KeyCode::Char('d') => {
                if let Some(selected) = state.chat_scroll_state.selected() {
                    if selected < state.messages.len() {
                        state.messages.remove(selected);
                    }
                }
            }
            _ => {}
        }
    }
}

fn handle_models_input(state: &mut AppState, key: KeyCode, shared_state: &SharedState) {
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
        KeyCode::Char('d') => {
            if let Some(selected) = state.model_list_state.selected() {
                if let Some(model) = state.models.get(selected) {
                    let model_name = model.name.clone();
                    let s = shared_state.clone();

                    state.status_message = Some(format!("Deleting {}...", model_name));

                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            let client = OllamaClient::new(None);
                            match client.delete_model(&model_name).await {
                                Ok(_) => {
                                    let mut s = s.lock().await;
                                    s.models.retain(|m| m.name != model_name);
                                    if s.selected_model.as_ref() == Some(&model_name) {
                                        s.selected_model = None;
                                    }
                                    s.status_message = Some(format!("Deleted {}", model_name));
                                }
                                Err(e) => {
                                    let mut s = s.lock().await;
                                    s.status_message = Some(format!("Delete failed: {}", e));
                                }
                            }
                        });
                    });
                }
            }
        }
        KeyCode::Char('r') => {
            let s = shared_state.clone();
            state.status_message = Some("Refreshing models...".to_string());
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    refresh_models(&s).await;
                    let mut s = s.lock().await;
                    s.status_message = Some("Models refreshed".to_string());
                });
            });
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
                state.search_list_state.select(Some(state.search_results.len() - 1));
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
