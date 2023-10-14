use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    default, io,
    os::unix::fs::{self},
    panic,
    path::Path,
    path::PathBuf,
    str::FromStr,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{self, Block, Borders, Cell, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};

use console::{Key, Term};

#[derive(Clone)]
pub enum Action {
    Back,
    Root,
}

#[derive(Clone)]
pub enum Entity {
    TextFile(PathBuf),
    Folder(PathBuf),
    Action(Action),
}

#[derive(Clone)]
pub enum Respond {
    Text(String),
    None,
}

pub struct FileManager {
    root: PathBuf,
    current: PathBuf,
    entities: Vec<Entity>,
    selected: Option<usize>,
}

impl FileManager {
    fn open_dir<T: AsRef<Path>>(dir: &T) -> Result<Vec<PathBuf>, io::Error> {
        let items = std::fs::read_dir(dir)?;
        let file_names: Vec<PathBuf> = items
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                Some(path)
            })
            .collect();

        Ok(file_names)
    }

    fn create_entities(files: Vec<PathBuf>) -> Vec<Entity> {
        let mut entities: Vec<Entity> = files
            .iter()
            .filter_map(|path| {
                if path.is_file() {
                    Some(Entity::TextFile(path.clone()))
                } else if path.is_dir() {
                    Some(Entity::Folder(path.clone()))
                } else {
                    None
                }
            })
            .collect();
        entities.push(Entity::Action(Action::Back));
        entities.push(Entity::Action(Action::Root));

        entities
    }

    fn goto_dir(&mut self, dir: PathBuf) -> Result<(), io::Error> {
        let files = Self::open_dir(&dir)?;
        self.entities = Self::create_entities(files);
        self.selected = None;
        self.current = dir;

        Ok(())
    }
}

impl FileManager {
    pub fn new(root: &str) -> Result<Self, io::Error> {
        let files = Self::open_dir(&root)?;

        Ok(Self {
            current: PathBuf::from(root),
            root: PathBuf::from(root),
            entities: Self::create_entities(files),
            selected: Option::default(),
        })
    }

    pub fn get_root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn get_entities(&self) -> Vec<Entity> {
        self.entities.clone()
    }

    pub fn get_selected_id(&self) -> Option<usize> {
        self.selected.clone()
    }

    pub fn get_selected_entity(&self) -> Option<Entity> {
        self.selected.map(|id| self.entities[id].clone())
    }

    pub fn next(&mut self) {
        if !self.entities.is_empty() {
            self.selected = match self.selected {
                Some(value) => Some((value + 1) % self.entities.len()),
                None => Some(0),
            };
        }
    }

    pub fn previous(&mut self) {
        if !self.entities.is_empty() {
            self.selected = match self.selected {
                Some(0) => Some(self.entities.len() - 1),
                Some(value) => Some(value - 1),
                None => Some(self.entities.len() - 1),
            };
        }
    }

    pub fn select(&mut self, id: usize) -> bool {
        if id < self.entities.len() {
            self.selected = Some(id);
            true
        } else {
            false
        }
    }

    pub fn action(&mut self) -> Result<Respond, io::Error> {
        self.selected
            .map_or(Ok(Respond::None), |id| match &self.entities[id] {
                Entity::TextFile(path) => Ok(Respond::Text(std::fs::read_to_string(path)?)),
                Entity::Folder(path) => {
                    Self::goto_dir(self, path.clone())?;
                    Ok(Respond::None)
                }
                Entity::Action(act) => {
                    match act {
                        Action::Back => {
                            let parent_path = self.current.parent().map(|path| PathBuf::from(path));
                            match parent_path {
                                Some(path) => Self::goto_dir(self, path)?,
                                None => (),
                            }
                        }
                        Action::Root => Self::goto_dir(self, self.root.clone())?,
                    }
                    Ok(Respond::None)
                }
            })
    }
}

enum Mode {
    Manager,
    Viewer,
}

fn draw_text<B: Backend>(frame: &mut Frame<B>, area: Rect, text_data: &String, scroll: u16) {
    let text = Text::from(text_data.as_str());
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Text").borders(Borders::ALL))
        .wrap(widgets::Wrap { trim: true })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area)
}

fn draw_block<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let block = Block::default().title("name").borders(Borders::ALL);
    frame.render_widget(block, area);
}

fn draw_list<B: Backend>(
    frame: &mut Frame<B>,
    area: Rect,
    list_data: &Vec<Entity>,
    list_state: Option<usize>,
) {
    let items: Vec<ListItem> = list_data
        .iter()
        .map(|entity| match entity {
            Entity::TextFile(path) => {
                ListItem::new(path.to_str().map_or("Unknown TextFile", |str| str))
                    .style(Style::default().fg(Color::White))
            }
            Entity::Folder(path) => {
                ListItem::new(path.to_str().map_or("Unknown Folder", |str| str))
                    .style(Style::default().fg(Color::Blue))
            }
            Entity::Action(act) => match act {
                Action::Back => ListItem::new("Back").style(Style::default().fg(Color::Red)),
                Action::Root => ListItem::new("Root").style(Style::default().fg(Color::Red)),
            },
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().title("List").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut state = ListState::default();
    state.select(list_state);
    frame.render_stateful_widget(list, area, &mut state);
}

fn run_session(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), io::Error> {
    let mut manager = FileManager::new("./")?;
    let mut mode = Mode::Manager;
    let mut text_data = String::new();
    let mut text_scroll: u16 = 0;

    // Render loop.
    let stdout = Term::stdout();
    loop {
        // Rendering.
        terminal.draw(|f: &mut Frame<'_, CrosstermBackend<io::Stdout>>| {
            let vertical_chunks = Layout::default()
                .direction(tui::layout::Direction::Vertical)
                .constraints([
                    Constraint::Percentage(10),
                    Constraint::Percentage(80),
                    Constraint::Percentage(10),
                ])
                .split(f.size());
            let horizontal_chunks = Layout::default()
                .direction(tui::layout::Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(vertical_chunks[1]);
            draw_list(
                f,
                horizontal_chunks[0],
                &manager.get_entities(),
                manager.get_selected_id(),
            );
            draw_text(f, horizontal_chunks[1], &text_data, text_scroll);
        })?;

        // Handling input.
        let key = stdout.read_key()?;
        match mode {
            Mode::Manager => match key {
                Key::Escape => break Ok(()),
                Key::ArrowUp => manager.previous(),
                Key::ArrowDown => manager.next(),
                Key::Enter => match manager.action()? {
                    Respond::Text(text) => {
                        text_data = text;
                        mode = Mode::Viewer;
                    }
                    Respond::None => (),
                },
                _ => (),
            },
            Mode::Viewer => match key {
                Key::ArrowUp => {
                    text_scroll = text_scroll
                        .checked_sub(1)
                        .map_or(text_scroll, |scroll| scroll)
                }
                Key::ArrowDown => {
                    text_scroll = text_scroll
                        .checked_add(1)
                        .map_or(text_scroll, |scroll| scroll)
                }
                _ => {
                    text_data = String::new();
                    mode = Mode::Manager
                }
            },
        }
    }
}

fn main() {
    // Initialize terminal for the session.
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("Cannot create a terminal");
    enable_raw_mode().expect("Cannot enable raw mode");
    execute!(terminal.backend_mut(), EnterAlternateScreen).expect("Cannot enable alternate screen");

    // Session.
    let result = run_session(&mut terminal);

    // Shutdown the session.
    disable_raw_mode().expect("Cannot disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .expect("Cannot disable alternate screen");
    match result {
        Ok(()) => {
            println!("End of the session")
        }
        Err(error) => {
            println!("Error {:?} ocurred in the session", error)
        }
    };
}
