use clap::Parser;
use crossterm::{
    event::{read, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    cmp::Reverse,
    io,
    path::Path,
    path::PathBuf,
    time::SystemTime,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Text},
    widgets::{self, Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};

#[derive(Clone)]
pub enum Action {
    Back,
    Root,
}

#[derive(Clone)]
pub enum ManagerEntity {
    TextFile(PathBuf),
    Folder(PathBuf),
    Action(Action),
}

#[derive(Clone)]
pub enum Respond {
    Text(String),
    Bin(Vec<u8>),
    None,
}

pub struct FileManager {
    root: PathBuf,
    current: PathBuf,
    entities: Vec<ManagerEntity>,
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

    fn create_entities(files: Vec<PathBuf>) -> Vec<ManagerEntity> {
        let mut entities: Vec<ManagerEntity> = files
            .iter()
            .filter_map(|path| {
                if path.is_file() {
                    Some(ManagerEntity::TextFile(path.clone()))
                } else if path.is_dir() {
                    Some(ManagerEntity::Folder(path.clone()))
                } else {
                    None
                }
            })
            .collect();
        entities.sort_by_cached_key(|entity| match entity {
            ManagerEntity::TextFile(path) => Reverse(path.metadata().map_or(None, |meta| {
                Some(meta.modified().map_or(SystemTime::UNIX_EPOCH, |st| st))
            })),
            ManagerEntity::Folder(path) => Reverse(path.metadata().map_or(None, |meta| {
                Some(meta.modified().map_or(SystemTime::UNIX_EPOCH, |st| st))
            })),
            ManagerEntity::Action(_act) => Reverse(None),
        });
        entities.push(ManagerEntity::Action(Action::Back));
        entities.push(ManagerEntity::Action(Action::Root));

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

    pub fn get_current(&self) -> PathBuf {
        self.current.clone()
    }

    pub fn get_entities_ref(&self) -> &Vec<ManagerEntity> {
        &self.entities
    }

    pub fn get_selected_id(&self) -> Option<usize> {
        self.selected.clone()
    }

    pub fn get_selected_entity(&self) -> Option<ManagerEntity> {
        self.selected.map(|id| self.entities[id].clone())
    }

    pub fn get_selected_entity_name(&self) -> Option<String> {
        self.selected.map_or(None, |id| match &self.entities[id] {
            ManagerEntity::TextFile(path) => path.file_name().map_or(None, |name| {
                name.to_owned().into_string().map_or(None, |str| Some(str))
            }),
            ManagerEntity::Folder(path) => path.file_name().map_or(None, |name| {
                name.to_owned().into_string().map_or(None, |str| Some(str))
            }),
            ManagerEntity::Action(_act) => None,
        })
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
                ManagerEntity::TextFile(path) => {
                    let text = std::fs::read_to_string(path);
                    match text {
                        Ok(text) => Ok(Respond::Text(text)),
                        Err(_err) => Ok(Respond::Bin(std::fs::read(path)?)),
                    }
                }
                ManagerEntity::Folder(path) => {
                    Self::goto_dir(self, path.clone())?;
                    Ok(Respond::None)
                }
                ManagerEntity::Action(act) => {
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

#[derive(Clone)]
pub enum ViewerEntity {
    Text(String),
    Binary(Vec<u8>),
}

pub struct Viewer {
    name: Option<String>,
    entity: ViewerEntity,
    scroll: u16,
    key: String,
}

impl Viewer {
    fn crypt_add(c: i32, count: usize, key: &str) -> i32 {
        let crypt: Vec<_> = key.bytes().collect();
        (c + crypt[count] as i32) % 256
    }

    fn crypt_rm(c: i32, count: usize, key: &str) -> i32 {
        let crypt: Vec<_> = key.bytes().collect();
        if c < crypt[count] as i32 {
            c - crypt[count] as i32 + 256
        } else {
            c - crypt[count] as i32
        }
    }

    fn decrypt_binary(bin: &Vec<u8>, key: &str) -> Result<String, std::string::FromUtf8Error> {
        let mut text: Vec<u8> = Vec::new();
        let mut count: usize = 0;
        for byte in bin {
            let ch = Self::crypt_rm(*byte as i32, count, key);
            text.push(ch as u8);
            count = (count + 1) % 5;
        }

        String::from_utf8(text)
    }
}

impl Viewer {
    pub fn new(key: &str) -> Result<Viewer, io::Error> {
        if key.len() < 5 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid key"));
        }

        Ok(Viewer {
            name: None,
            entity: ViewerEntity::Text(String::new()),
            scroll: 0,
            key: key.to_string(),
        })
    }

    pub fn set_entity(&mut self, entity: ViewerEntity, name: Option<String>) {
        self.name = name;
        self.entity = entity;
        self.scroll = 0;
    }

    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    pub fn get_entity_ref(&self) -> &ViewerEntity {
        &self.entity
    }

    pub fn get_scroll(&self) -> u16 {
        self.scroll
    }

    pub fn scroll_up(&mut self, value: u16) {
        self.scroll = self
            .scroll
            .checked_sub(value)
            .map_or(self.scroll, |scroll| scroll)
    }

    pub fn scroll_down(&mut self, value: u16) {
        self.scroll = self
            .scroll
            .checked_add(value)
            .map_or(self.scroll, |scroll| scroll)
    }

    pub fn decrypt(&mut self) -> Result<(), std::string::FromUtf8Error> {
        match &self.entity {
            ViewerEntity::Text(_text) => Ok(()),
            ViewerEntity::Binary(bin) => {
                self.set_entity(
                    ViewerEntity::Text(Self::decrypt_binary(bin, self.key.as_str())?),
                    self.name.clone(),
                );
                Ok(())
            }
        }
    }

    pub fn clear(&mut self) {
        self.name = None;
        self.entity = ViewerEntity::Text(String::new());
        self.scroll = 0;
    }
}

#[derive(PartialEq)]
enum Mode {
    Manager,
    Viewer,
    Exit,
}

fn update(
    key: KeyCode,
    mode: Mode,
    manager: &mut FileManager,
    viewer: &mut Viewer,
) -> Result<Mode, io::Error> {
    match mode {
        Mode::Manager => match key {
            KeyCode::Esc => Ok(Mode::Exit),
            KeyCode::Up => {
                manager.previous();
                Ok(Mode::Manager)
            }
            KeyCode::Down => {
                manager.next();
                Ok(Mode::Manager)
            }
            KeyCode::Enter => match manager.action()? {
                Respond::Text(text) => {
                    viewer.set_entity(ViewerEntity::Text(text), manager.get_selected_entity_name());
                    Ok(Mode::Viewer)
                }
                Respond::Bin(bin) => {
                    viewer.set_entity(
                        ViewerEntity::Binary(bin),
                        manager.get_selected_entity_name(),
                    );
                    Ok(Mode::Viewer)
                }
                Respond::None => Ok(Mode::Manager),
            },
            _ => Ok(Mode::Manager),
        },
        Mode::Viewer => match key {
            KeyCode::Up => {
                viewer.scroll_up(1);
                Ok(Mode::Viewer)
            }
            KeyCode::Down => {
                viewer.scroll_down(1);
                Ok(Mode::Viewer)
            }
            KeyCode::Enter => {
                viewer
                    .decrypt()
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
                Ok(Mode::Viewer)
            }
            _ => {
                viewer.clear();
                Ok(Mode::Manager)
            }
        },
        Mode::Exit => Ok(Mode::Exit),
    }
}

fn draw_block<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let block = Block::default().title("Block").borders(Borders::ALL);
    frame.render_widget(block, area);
}

fn draw_viewer<B: Backend>(frame: &mut Frame<B>, area: Rect, viewer: &Viewer) {
    let entity = viewer.get_entity_ref();
    let text = match entity {
        ViewerEntity::Text(text) => Text::from(text.as_str()),
        ViewerEntity::Binary(_bin) => Text::from("It is binary file"),
    };
    let title = viewer
        .get_name()
        .map_or(String::from("Text File"), |name| name);
    let paragraph = Paragraph::new(text)
        .block(Block::default().title(title.as_str()).borders(Borders::ALL))
        .wrap(widgets::Wrap { trim: true })
        .scroll((viewer.get_scroll(), 0));
    frame.render_widget(paragraph, area)
}

fn draw_manager<B: Backend>(frame: &mut Frame<B>, area: Rect, manager: &FileManager) {
    let list_data = manager.get_entities_ref();
    let items: Vec<ListItem> = list_data
        .iter()
        .map(|entity| match entity {
            ManagerEntity::TextFile(path) => {
                ListItem::new(path.file_name().map_or("Unknown text file", |str| {
                    str.to_str().map_or("Unknown text name", |name| name)
                }))
                .style(Style::default().fg(Color::White))
            }
            ManagerEntity::Folder(path) => {
                ListItem::new(path.file_name().map_or("Unknown folder", |str| {
                    str.to_str().map_or("Unknown folder name", |name| name)
                }))
                .style(Style::default().fg(Color::Blue))
            }
            ManagerEntity::Action(act) => match act {
                Action::Back => ListItem::new("Back").style(Style::default().fg(Color::Red)),
                Action::Root => ListItem::new("Root").style(Style::default().fg(Color::Red)),
            },
        })
        .collect();
    let title = manager
        .get_current()
        .to_str()
        .map_or(String::from("Folder"), |name| String::from(name));
    let list = List::new(items)
        .block(Block::default().title(title.as_str()).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut state = ListState::default();
    state.select(manager.get_selected_id());
    frame.render_stateful_widget(list, area, &mut state);
}

fn run_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    root: &str,
    key: &str,
) -> Result<(), io::Error> {
    let mut manager = FileManager::new(root)?;
    let mut viewer = Viewer::new(key)?;
    let mut mode = Mode::Manager;

    // Render loop.
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
            draw_manager(f, horizontal_chunks[0], &manager);
            draw_viewer(f, horizontal_chunks[1], &viewer);
        })?;

        // Handling input.
        if let Event::Key(key) = read()? {
            mode = update(key.code, mode, &mut manager, &mut viewer)?;
        }

        if mode == Mode::Exit {
            break Ok(());
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Root directory.
    #[arg(long)]
    root: String,
}

fn main() {
    // Parse CLI arguments.
    let args = Args::parse();

    // Password.
    println!("Type the session password");
    let password = rpassword::read_password().expect("Password is expected");

    // Initialize terminal for the session.
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("Cannot create a terminal");
    enable_raw_mode().expect("Cannot enable raw mode");
    execute!(terminal.backend_mut(), EnterAlternateScreen).expect("Cannot enable alternate screen");

    // Session.
    let result = run_session(&mut terminal, args.root.as_str(), password.as_str());

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
