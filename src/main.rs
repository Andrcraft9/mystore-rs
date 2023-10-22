use chrono::Utc;
use clap::Parser;
use crossterm::{
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    cmp::Reverse,
    fmt,
    fs::File,
    io::{self, Write},
    path::Path,
    path::PathBuf,
    time::SystemTime,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{self, Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use tui_textarea::TextArea;

#[derive(Clone, PartialEq)]
pub enum Action {
    Back,
    Root,
}

#[derive(Clone, PartialEq)]
pub enum ManagerEntity {
    TextFile(PathBuf),
    Folder(PathBuf),
    Action(Action),
}

#[derive(Clone, PartialEq)]
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
    created_entities: Vec<ManagerEntity>,
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

    fn create_entities(files: Vec<PathBuf>, is_root: bool) -> Vec<ManagerEntity> {
        let mut folder_entities: Vec<ManagerEntity> = files
            .iter()
            .filter_map(|path| {
                if path.is_dir() {
                    Some(ManagerEntity::Folder(path.clone()))
                } else {
                    None
                }
            })
            .collect();
        folder_entities.sort_by_cached_key(|entity| match entity {
            ManagerEntity::TextFile(path) => Some(path.as_path().to_owned()),
            ManagerEntity::Folder(path) => Some(path.as_path().to_owned()),
            ManagerEntity::Action(_act) => None,
        });

        let mut file_entities: Vec<ManagerEntity> = files
            .iter()
            .filter_map(|path| {
                if path.is_file() {
                    Some(ManagerEntity::TextFile(path.clone()))
                } else {
                    None
                }
            })
            .collect();
        file_entities.sort_by_cached_key(|entity| match entity {
            ManagerEntity::TextFile(path) => Reverse(path.metadata().map_or(None, |meta| {
                Some(meta.modified().map_or(SystemTime::UNIX_EPOCH, |st| st))
            })),
            ManagerEntity::Folder(path) => Reverse(path.metadata().map_or(None, |meta| {
                Some(meta.modified().map_or(SystemTime::UNIX_EPOCH, |st| st))
            })),
            ManagerEntity::Action(_act) => Reverse(None),
        });

        let mut entities = folder_entities;
        entities.extend(file_entities);

        if !is_root {
            entities.push(ManagerEntity::Action(Action::Back));
            entities.push(ManagerEntity::Action(Action::Root));
        }

        entities
    }

    fn goto_dir(&mut self, dir: PathBuf) -> Result<(), io::Error> {
        let is_root = dir == self.root;
        let files = Self::open_dir(&dir)?;
        self.entities = Self::create_entities(files, is_root);
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
            entities: Self::create_entities(files, true),
            selected: Option::default(),
            created_entities: Vec::new(),
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

    pub fn refresh(&mut self) -> Result<(), io::Error> {
        let selected = self.selected;
        Self::goto_dir(self, self.current.clone())?;
        selected.map(|id| Self::select(self, id));

        Ok(())
    }

    pub fn create_file(
        &mut self,
        data: Vec<u8>,
        file_name: Option<String>,
    ) -> Result<(), io::Error> {
        let file_name = file_name.map_or(Utc::now().to_rfc3339(), |name| name);
        let file_path = self.current.join(file_name);
        let mut file = File::create(file_path.clone())?;
        file.write_all(&data)?;

        self.created_entities
            .push(ManagerEntity::TextFile(file_path));
        self.refresh()?;

        Ok(())
    }

    pub fn delete_selected(&mut self) -> Result<(), io::Error> {
        self.selected
            .map_or(Ok(()), |id| match &self.entities[id] {
                ManagerEntity::TextFile(path) => self
                    .created_entities
                    .iter()
                    .position(|elem| *elem == ManagerEntity::TextFile(path.clone()))
                    .map_or(
                        Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Cannot delete the entity not created in the current session",
                        )),
                        |item| {
                            std::fs::remove_file(path.clone())?;
                            self.created_entities.remove(item);
                            Ok(())
                        },
                    ),
                ManagerEntity::Folder(_path) => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot delete the folder entity",
                )),
                ManagerEntity::Action(_act) => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot delete the action entity",
                )),
            })?;

        self.refresh()?;

        Ok(())
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

#[derive(Clone, PartialEq)]
pub enum ViewerEntity {
    Text(String),
    DecryptedText(String),
    Binary(Vec<u8>),
}

pub struct Viewer {
    name: Option<String>,
    entity: ViewerEntity,
    scroll: u16,
    key: String,
}

impl Viewer {
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
        self.scroll = 0;
        match entity {
            ViewerEntity::Text(_) => self.entity = entity,
            ViewerEntity::DecryptedText(_) => self.entity = entity,
            ViewerEntity::Binary(bin) => {
                // Try to decrypt binary:
                let decrypted = Self::decrypt_binary(&bin, self.key.as_str());
                match decrypted {
                    Ok(text) => self.entity = ViewerEntity::DecryptedText(text),
                    Err(_) => self.entity = ViewerEntity::Binary(bin),
                }
            }
        }
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

    pub fn clear(&mut self) {
        self.name = None;
        self.entity = ViewerEntity::Text(String::new());
        self.scroll = 0;
    }
}

pub struct Editor<'a> {
    textarea: Option<TextArea<'a>>,
    key: String,
}

impl Editor<'_> {
    fn crypt_add(c: i32, count: usize, key: &str) -> i32 {
        let crypt: Vec<_> = key.bytes().collect();
        (c + crypt[count] as i32) % 256
    }

    fn encrypt_string(str: &String, key: &str) -> Vec<u8> {
        let mut encrypt_text: Vec<u8> = Vec::new();
        let mut count: usize = 0;
        for byte in str.as_bytes() {
            let ch = Self::crypt_add(*byte as i32, count, key);
            encrypt_text.push(ch as u8);
            count = (count + 1) % 5;
        }

        encrypt_text
    }
}

impl<'a> Editor<'a> {
    pub fn new(key: &str) -> Editor<'a> {
        Editor {
            textarea: None,
            key: key.to_string(),
        }
    }

    pub fn init(&mut self) {
        self.textarea = Some(TextArea::default());
    }

    pub fn get_textarea_ref(&self) -> Option<&TextArea<'a>> {
        self.textarea.as_ref()
    }

    pub fn get_textarea_mut(&mut self) -> Option<&mut TextArea<'a>> {
        self.textarea.as_mut()
    }

    pub fn finish(&mut self) -> Result<String, io::Error> {
        if let Some(textarea) = self.textarea.take() {
            return Ok(textarea.into_lines().join("\n"));
        }

        Ok(String::new())
    }

    pub fn finish_encrypt(&mut self) -> Result<Vec<u8>, io::Error> {
        if let Some(textarea) = self.textarea.take() {
            let text = textarea.into_lines().join("\n");
            let encrypted_text = Self::encrypt_string(&text, self.key.as_str());
            return Ok(encrypted_text);
        }

        Ok(Vec::new())
    }
}

#[derive(Clone, PartialEq)]
enum Mode {
    Manager,
    Viewer,
    Editor,
    Exit,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Mode::Manager => {
                let help_manager = vec![
                    String::from("Esc: Quit, end the session"),
                    String::from("Down: Select next item"),
                    String::from("Up: Select previous item"),
                    String::from("Enter: Action on the selected item"),
                    String::from("E: Open the editor"),
                    String::from("N: Create a new editor instance"),
                    String::from("D: Delete the selected item"),
                ];
                write!(f, "Manager mode\n{}", help_manager.join("; "))
            }
            Mode::Viewer => {
                let help_viewer = vec![
                    String::from("Esc: Quit"),
                    String::from("Down, Up: Scroll the viewer"),
                ];
                write!(f, "Viewer mode\n{}", help_viewer.join("; "))
            }
            Mode::Editor => {
                let help_editor = vec![
                    String::from("Esc: Quit"),
                    String::from("Ctrl + S: Save the text file"),
                    String::from("Ctrl + E: Encrypt, and save the encrypted file"),
                    String::from("Other: See TextArea help"),
                ];
                write!(f, "Editor mode\n{}", help_editor.join("; "))
            }
            Mode::Exit => write!(f, "End the session"),
        }
    }
}

fn update(
    key: KeyEvent,
    mode: Mode,
    manager: &mut FileManager,
    viewer: &mut Viewer,
    editor: &mut Editor,
) -> Result<Mode, io::Error> {
    match mode {
        Mode::Manager => match key.code {
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
            KeyCode::Char('e') | KeyCode::Char('E') => Ok(Mode::Editor),
            KeyCode::Char('n') | KeyCode::Char('N') => {
                editor.init();
                Ok(Mode::Editor)
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                manager.delete_selected()?;
                Ok(Mode::Manager)
            }
            _ => Ok(Mode::Manager),
        },
        Mode::Viewer => match key.code {
            KeyCode::Up => {
                viewer.scroll_up(1);
                Ok(Mode::Viewer)
            }
            KeyCode::Down => {
                viewer.scroll_down(1);
                Ok(Mode::Viewer)
            }
            _ => {
                viewer.clear();
                Ok(Mode::Manager)
            }
        },
        Mode::Editor => match key {
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: _,
                kind: _,
                state: _,
            } => Ok(Mode::Manager),
            KeyEvent {
                code: KeyCode::Char('s') | KeyCode::Char('S'),
                modifiers: KeyModifiers::CONTROL,
                kind: _,
                state: _,
            } => {
                let text = editor.finish()?;
                manager.create_file(text.into_bytes(), None)?;
                Ok(Mode::Manager)
            }
            KeyEvent {
                code: KeyCode::Char('e') | KeyCode::Char('E'),
                modifiers: KeyModifiers::CONTROL,
                kind: _,
                state: _,
            } => {
                let encrypted = editor.finish_encrypt()?;
                manager.create_file(encrypted, None)?;
                Ok(Mode::Manager)
            }
            _ => {
                editor
                    .get_textarea_mut()
                    .map(|textarea: &mut TextArea<'_>| textarea.input(key));
                Ok(Mode::Editor)
            }
        },
        Mode::Exit => Ok(Mode::Exit),
    }
}

fn draw_session_status<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let paragraph = Paragraph::new(Utc::now().to_rfc2822())
        .block(Block::default().title("Session").borders(Borders::ALL));
    frame.render_widget(paragraph, area)
}

fn draw_help<B: Backend>(frame: &mut Frame<B>, area: Rect, mode: &Mode) {
    let paragraph = Paragraph::new(mode.to_string())
        .block(Block::default().borders(Borders::ALL))
        .wrap(widgets::Wrap { trim: false });
    frame.render_widget(paragraph, area)
}

fn draw_error<B: Backend>(frame: &mut Frame<B>, area: Rect, err: &io::Error) {
    let paragraph = Paragraph::new(err.to_string())
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Red))
        .wrap(widgets::Wrap { trim: true });
    frame.render_widget(paragraph, area)
}

fn draw_viewer<B: Backend>(frame: &mut Frame<B>, area: Rect, viewer: &Viewer) {
    let entity = viewer.get_entity_ref();
    let paragraph = match entity {
        ViewerEntity::Text(text) => {
            let text = Text::from(text.as_str());
            let title = viewer
                .get_name()
                .map_or(String::from("Text File"), |name| name);
            Paragraph::new(text)
                .block(
                    Block::default()
                        .border_style(Style::default().fg(Color::White))
                        .title(title)
                        .borders(Borders::ALL),
                )
                .wrap(widgets::Wrap { trim: true })
                .scroll((viewer.get_scroll(), 0))
        }
        ViewerEntity::DecryptedText(text) => {
            let text = Text::from(text.as_str());
            let title = viewer
                .get_name()
                .map_or(String::from("Encrypted File"), |name| name);
            Paragraph::new(text)
                .block(
                    Block::default()
                        .border_style(Style::default().fg(Color::Blue))
                        .title(title)
                        .borders(Borders::ALL),
                )
                .wrap(widgets::Wrap { trim: true })
                .scroll((viewer.get_scroll(), 0))
        }
        ViewerEntity::Binary(_bin) => {
            let text = Text::from("Binary file");
            let title = viewer
                .get_name()
                .map_or(String::from("Binary File"), |name| name);
            Paragraph::new(text)
                .block(
                    Block::default()
                        .border_style(Style::default().fg(Color::Red))
                        .title(title)
                        .borders(Borders::ALL),
                )
                .wrap(widgets::Wrap { trim: true })
        }
    };
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
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        );
    let mut state = ListState::default();
    state.select(manager.get_selected_id());
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_editor<B: Backend>(frame: &mut Frame<B>, area: Rect, editor: &Editor) {
    editor.get_textarea_ref().map(|textarea| {
        let widget = textarea.widget();
        frame.render_widget(widget, area);
    });
}

fn run_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    root: &str,
    key: &str,
) -> Result<(), io::Error> {
    let mut manager = FileManager::new(root)?;
    let mut viewer = Viewer::new(key)?;
    let mut editor = Editor::new(key);
    let mut mode = Mode::Manager;
    let mut status: Result<(), io::Error> = Ok(());

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
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
                .split(vertical_chunks[1]);

            draw_session_status(f, vertical_chunks[0]);
            draw_manager(f, horizontal_chunks[0], &manager);
            if mode == Mode::Editor {
                draw_editor(f, horizontal_chunks[1], &editor);
            } else {
                draw_viewer(f, horizontal_chunks[1], &viewer);
            }
            if let Err(err) = &status {
                draw_error(f, vertical_chunks[2], &err);
            } else {
                draw_help(f, vertical_chunks[2], &mode);
            }
        })?;

        // Handling input.
        if let Event::Key(key) = read()? {
            match update(key, mode.clone(), &mut manager, &mut viewer, &mut editor) {
                Ok(new_mode) => {
                    status = Ok(());
                    mode = new_mode;
                }
                Err(err) => status = Err(err),
            }
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
