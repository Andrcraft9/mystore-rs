use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    default, io,
    os::unix::fs::{self},
    panic,
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

pub struct ListData {
    pub items: Vec<String>,
    state: Option<usize>,
}

impl ListData {
    pub fn new(items: Vec<String>) -> ListData {
        ListData {
            items: items,
            state: Option::default(),
        }
    }

    pub fn get_state(&self) -> Option<usize> {
        self.state
    }

    pub fn select(&mut self, id: usize) {
        if (id < self.items.len()) {
            self.state = Some(id);
        }
    }

    pub fn next(&mut self) {
        self.state = match self.state {
            Some(value) => Some((value + 1) % self.items.len()),
            None => Some(0),
        };
    }

    pub fn previous(&mut self) {
        self.state = match self.state {
            Some(0) => Some(self.items.len() - 1),
            Some(value) => Some(value - 1),
            None => Some(self.items.len() - 1),
        };
    }
}

/*
pub struct MenuList<'a> {
    items: Vec<ListItem<'a>>,
    state: ListState
}

impl<'a> MenuList<'a> {
    pub fn new(items: Vec<ListItem<'a>>) -> MenuList<'a> {
        //let list_items: Vec<ListItem> = items.iter().map(|i| ListItem::new(i.as_ref())).collect();
        MenuList { items: items, state: ListState::default() }
    }
}

impl MenuList<'_> {
    pub fn add_item(&mut self, item: String) {
        let list_item = ListItem::new(item);
        self.items.push(list_item)
    }

    pub fn draw<B: Backend>(&mut self, frame: &mut Frame<B>) {
        let text = Text::from("Starting menu");

        let list = List::new(self.items.clone())
            .block(Block::default()
            .title("List")
            .borders(Borders::ALL))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol(">>");

        let margin = Margin{vertical: 1, horizontal: 1};
        let inner_frame = frame.size().inner(&margin);
        frame.render_stateful_widget(list, inner_frame, &mut self.state)
    }
}
*/

fn draw_text<B: Backend>(frame: &mut Frame<B>, area: Rect, text_data: &String) {
    let text = Text::from(text_data.as_str());
    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, area)
}

fn draw_block<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let block = Block::default().title("name").borders(Borders::ALL);
    frame.render_widget(block, area);
}

fn draw_list<B: Backend>(frame: &mut Frame<B>, area: Rect, list_data: &ListData) {
    let items: Vec<ListItem> = list_data
        .items
        .iter()
        .map(|i| ListItem::new(i.as_ref()))
        .collect();
    let list = List::new(items)
        .block(Block::default().title("List").borders(Borders::ALL))
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
        .highlight_symbol(">>");
    let mut state = ListState::default();
    state.select(list_data.get_state());
    frame.render_stateful_widget(list, area, &mut state);
}

fn run_session(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), io::Error> {
    let items = std::fs::read_dir("./")?;

    // Extract the filenames from the directory entries and store them in a vector
    let file_names: Vec<String> = items
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.is_file() {
                path.file_name()?.to_str().map(|s| s.to_owned())
            } else {
                None
            }
        })
        .collect();

    let mut list_data = ListData::new(file_names);
    let mut text_data = String::new();

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
            draw_list(f, horizontal_chunks[0], &list_data);
            draw_text(f, horizontal_chunks[1], &text_data);
        })?;

        // Handling input.
        let key = stdout.read_key()?;
        match key {
            Key::Escape => break Ok(()),
            Key::ArrowUp => list_data.previous(),
            Key::ArrowDown => list_data.next(),
            Key::Enter => text_data = String::from("Hello\nIt\nis\nText\nto\nModify and replace"),
            Key::Backspace => text_data = String::new(),
            _ => (),
        }
    }
    //let items = vec![String::from_str("First item").unwrap(), String::from_str("Item 2").unwrap()];
    //let menu_items = items.iter().map(|i| ListItem::new(i.as_ref())).collect();
    //let mut menu = MenuList::new(menu_items);
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
