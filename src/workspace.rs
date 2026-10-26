//! Two independent log sessions with a shared terminal and explicit focus.
use crate::{
    app::App,
    ui::{FlikUi, menu::MenuAction, modals::ActiveModal},
};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::{
    Terminal,
    backend::Backend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use std::{
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum OpenTarget {
    Active,
    Other,
    Command,
}
struct OpenDialog {
    path: String,
    target: OpenTarget,
    error: Option<String>,
}

pub struct Workspace {
    pub panes: Vec<App>,
    pub active: usize,
    open_dialog: Option<OpenDialog>,
    pub(crate) view: Option<InvestigationView>,
    job: Option<InvestigationJob>,
    history: Vec<String>,
    history_cursor: usize,
    context_radius: usize,
    redaction_keys: Vec<String>,
}

impl Workspace {
    pub fn new(app: App) -> Self {
        Self {
            panes: vec![app],
            active: 0,
            open_dialog: None,
            view: None,
            job: None,
            history: Vec::new(),
            history_cursor: 0,
            context_radius: 20,
            redaction_keys: Vec::new(),
        }
    }

    /// Open first, then replace: a failed open never destroys an existing session.
    pub fn open_other(&mut self, path: &Path) -> io::Result<()> {
        let mut app = App::new(path.to_path_buf())?;
        self.cancel_job();
        self.view = None;
        app.mouse_capture_enabled = self.panes[self.active].mouse_capture_enabled;
        if self.panes.len() == 1 {
            self.panes.push(app);
            self.active = 1;
        } else {
            self.active = 1 - self.active;
            self.panes[self.active] = app;
        }
        Ok(())
    }

    pub fn close_active(&mut self) {
        self.cancel_job();
        self.view = None;
        if self.panes.len() == 2 {
            self.panes.remove(self.active);
            self.active = 0;
        } else {
            self.panes[0].ui_state.status_toast = Some((
                "Use Q to quit; keep at least one file open".into(),
                Instant::now(),
            ));
        }
    }

    /// Focus belongs to the workspace, independent of a pane's local editor/menu.
    fn focus_pane(&mut self, index: usize) {
        if self.panes.len() < 2 {
            self.panes[self.active].ui_state.status_toast = Some((
                "Open a second file with Ctrl+N to switch panes".into(),
                Instant::now(),
            ));
            return;
        }
        if index >= self.panes.len() {
            return;
        }
        self.panes[self.active].menu.close();
        self.panes[self.active].last_click = None;
        self.panes[self.active].last_q_press = None;
        self.panes[self.active].ui_state.quit_warning = false;
        self.active = index;
        self.panes[index].last_click = None;
        self.panes[index].last_q_press = None;
        self.panes[index].ui_state.quit_warning = false;
        self.view = None;
    }

    fn show_open(&mut self, target: OpenTarget) {
        self.open_dialog = Some(OpenDialog {
            path: String::new(),
            target,
            error: None,
        });
    }

    // Keep side-by-side panels readable; narrow terminals stack the panels vertically.
    fn pane_areas(&self, area: Rect) -> Vec<(usize, Rect)> {
        let body = Rect {
            height: area.height.saturating_sub(1),
            ..area
        };
        if self.panes.len() == 1 {
            return vec![(0, body)];
        }
        if area.width < 100 && area.height < 20 {
            return vec![(self.active, body)];
        }
        let direction = if area.width >= 100 {
            Direction::Horizontal
        } else {
            Direction::Vertical
        };
        Layout::default()
            .direction(direction)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(body)
            .iter()
            .copied()
            .enumerate()
            .collect()
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        for (index, pane_area) in self.pane_areas(area) {
            if pane_area.width == 0 || pane_area.height == 0 {
                continue;
            }
            let pane = &self.panes[index];
            let active = self.active == index;
            let style = Style::default()
                .fg(if active { Color::Black } else { Color::Gray })
                .bg(if active { Color::Cyan } else { Color::DarkGray })
                .add_modifier(Modifier::BOLD);
            let label = format!(
                " {} [{}] {}",
                if active { ">" } else { " " },
                index + 1,
                pane.reader.path.display()
            );
            Paragraph::new(label).style(style).render(
                Rect {
                    height: 1,
                    ..pane_area
                },
                buf,
            );
            let local = Rect::new(0, 0, pane_area.width, pane_area.height.saturating_sub(1));
            // Existing pane widgets and mouse handling use pane-local coordinates.
            if local.width < 12 || local.height < 6 {
                continue;
            }
            let mut pane_buffer = Buffer::empty(local);
            FlikUi::render(
                local,
                &mut pane_buffer,
                &pane.theme,
                &pane.reader,
                &pane.filter,
                &pane.format,
                &pane.menu,
                if active {
                    &pane.modal
                } else {
                    &ActiveModal::None
                },
                &pane.ui_state,
            );
            for y in 0..local.height {
                for x in 0..local.width {
                    if let Some(cell) = buf.cell_mut((pane_area.x + x, pane_area.y + 1 + y)) {
                        *cell = pane_buffer[(x, y)].clone();
                    }
                }
            }
        }
        if area.height > 0 {
            Paragraph::new(
                " Ctrl+] / F12 Pane | Click pane to focus | Ctrl+N Add | Ctrl+K Investigate | Ctrl+O Open | Ctrl+W Close ",
            )
            .style(Style::default().fg(Color::Cyan))
            .render(Rect::new(area.x, area.bottom() - 1, area.width, 1), buf);
        }
        if let Some(view) = &self.view {
            self.render_investigation(view, area, buf);
        }
        if let Some(job) = &self.job {
            let progress = format!(
                " {}: {} / {} lines scanned — Esc cancels",
                job.label,
                job.scanned.load(std::sync::atomic::Ordering::Relaxed),
                job.total
            );
            Paragraph::new(progress)
                .style(Style::default().fg(Color::Yellow).bg(Color::Black))
                .render(
                    Rect::new(area.x, area.y, area.width, area.height.min(1)),
                    buf,
                );
        }
        if let Some(dialog) = &self.open_dialog {
            let width = area.width.min(90);
            let height = area
                .height
                .min(if matches!(dialog.target, OpenTarget::Command) {
                    15
                } else {
                    8
                });
            let popup = Rect::new(
                area.x + (area.width - width) / 2,
                area.y + (area.height - height) / 2,
                width,
                height,
            );
            Clear.render(popup, buf);
            let title = match dialog.target {
                OpenTarget::Active => " Open in active pane ",
                OpenTarget::Other => " Add / replace other pane ",
                OpenTarget::Command => " Investigate — Ctrl+K commands ",
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().bg(Color::Black).fg(Color::White));
            let inner = block.inner(popup);
            block.render(popup, buf);
            let path_width = inner.width.saturating_sub(2) as usize;
            let tail: String = dialog
                .path
                .chars()
                .rev()
                .take(path_width)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let guide = if matches!(dialog.target, OpenTarget::Command) {
                "context 20 — before/after selected result
time START..END | jump TIME | around 5 | untime
Times: 2026-10-21T10:30:00Z or +02:00 (RFC3339)
histogram minute|hour|day — filtered occurrences\ncompare TIME [MIN] | compare files\nmerge | trace [ID] — applies to both files
save PATH.json | load PATH.json
export PATH.jsonl — context, sources, redaction
redact email,customer_id — extra keys to mask"
            } else {
                "Path (absolute, relative, or ~/):"
            };
            Paragraph::new(format!(
                "{guide}\n\n> {tail}_\n{}\nEnter: run   Esc: cancel   Up/Down: history",
                dialog.error.as_deref().unwrap_or("")
            ))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .render(inner, buf);
        }
    }

    fn dialog_key(&mut self, key: KeyEvent) {
        let Some(dialog) = self.open_dialog.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.open_dialog = None,
            KeyCode::Backspace => {
                dialog.path.pop();
                dialog.error = None;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                dialog.path.clear();
                dialog.error = None;
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                dialog.path.push(c);
                dialog.error = None;
            }
            KeyCode::Up | KeyCode::Down if matches!(dialog.target, OpenTarget::Command) => {
                if key.code == KeyCode::Up {
                    self.history_cursor = self.history_cursor.saturating_sub(1);
                } else {
                    self.history_cursor = (self.history_cursor + 1).min(self.history.len());
                }
                dialog.path = self
                    .history
                    .get(self.history_cursor)
                    .cloned()
                    .unwrap_or_default();
            }
            KeyCode::Enter => {
                if matches!(dialog.target, OpenTarget::Command) {
                    let command = dialog.path.clone();
                    match self.execute_command(&command) {
                        Ok(()) => {
                            self.history.push(command);
                            if self.history.len() > 100 {
                                self.history.remove(0);
                            }
                            self.open_dialog = None;
                        }
                        Err(err) => self.open_dialog.as_mut().unwrap().error = Some(err),
                    }
                    return;
                }
                let path = expand_path(&dialog.path);
                let target = dialog.target;
                let result = match target {
                    OpenTarget::Command => unreachable!(),
                    OpenTarget::Other => self.open_other(&path),
                    OpenTarget::Active => App::new(path).map(|mut app| {
                        self.cancel_job();
                        self.view = None;
                        app.mouse_capture_enabled = self.panes[self.active].mouse_capture_enabled;
                        self.panes[self.active] = app;
                    }),
                };
                match result {
                    Ok(()) => self.open_dialog = None,
                    Err(err) => self.open_dialog.as_mut().unwrap().error = Some(err.to_string()),
                }
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, height: usize) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if self.open_dialog.is_some() {
            self.dialog_key(key);
            return;
        }
        // Resolve focus before pane-local menus, filters and investigation views.
        // Ctrl+] and F12 also work when the terminal reserves Ctrl+Arrow.
        let focus = match key.code {
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => Some(0),
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(self.panes.len().saturating_sub(1))
            }
            // Legacy terminals send Ctrl+] as 0x1d; crossterm 0.28 decodes
            // that byte as Ctrl+5. Accept both encodings of the same shortcut.
            KeyCode::Char(']' | '5') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some((self.active + 1) % self.panes.len())
            }
            KeyCode::F(12) => Some((self.active + 1) % self.panes.len()),
            _ => None,
        };
        if let Some(index) = focus {
            self.focus_pane(index);
            return;
        }
        if key.code == KeyCode::Esc && self.job.is_some() {
            self.cancel_job();
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.show_command("");
            return;
        }
        if self.view.is_some() {
            self.view_key(key, height);
            return;
        }
        let pane = &self.panes[self.active];
        if pane.modal == ActiveModal::None
            && !pane.menu.is_open
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            match key.code {
                KeyCode::Char('g') => {
                    self.run_command_or_error("context 20");
                    return;
                }
                KeyCode::Char('t') => {
                    self.show_command("time ");
                    return;
                }
                KeyCode::Char('l') => {
                    self.run_command_or_error("merge");
                    return;
                }
                KeyCode::Char('r') => {
                    self.show_command("trace ");
                    return;
                }
                KeyCode::Char('p') => {
                    self.show_command("save ");
                    return;
                }
                KeyCode::Char('e') => {
                    self.show_command("export ");
                    return;
                }
                KeyCode::Char('o') => {
                    self.show_open(OpenTarget::Active);
                    return;
                }
                KeyCode::Char('n') => {
                    self.show_open(OpenTarget::Other);
                    return;
                }
                KeyCode::Char('w') => {
                    self.close_active();
                    return;
                }
                _ => {}
            }
        }
        self.panes[self.active].handle_key(key, height);
        self.process_action();
    }

    fn process_action(&mut self) {
        match self.panes[self.active].pending_workspace_action.take() {
            Some(MenuAction::ShowHistogram) => self.run_command_or_error("histogram hour"),
            Some(MenuAction::Investigate) => self.show_command(""),
            Some(MenuAction::ShowContext) => self.run_command_or_error("context 20"),
            Some(MenuAction::TimeRange) => self.show_command("time "),
            Some(MenuAction::MergeFiles) => self.run_command_or_error("merge"),
            Some(MenuAction::TraceFiles) => self.show_command("trace "),
            Some(MenuAction::SaveSession) => self.show_command("save "),
            Some(MenuAction::LoadSession) => self.show_command("load "),
            Some(MenuAction::ExportIncident) => self.show_command("export "),
            Some(MenuAction::OpenFile) => self.show_open(OpenTarget::Active),
            Some(MenuAction::AddFile) => self.show_open(OpenTarget::Other),
            Some(MenuAction::ClosePane) => self.close_active(),
            Some(MenuAction::SwitchPane) => self.focus_pane((self.active + 1) % self.panes.len()),
            _ => {}
        }
        // Mouse capture belongs to the terminal, so synchronize the preference across sessions.
        let enabled = self.panes[self.active].mouse_capture_enabled;
        for pane in &mut self.panes {
            pane.mouse_capture_enabled = enabled;
        }
    }

    pub fn handle_mouse(&mut self, mut mouse: MouseEvent, area: Rect) {
        if self.open_dialog.is_some() {
            return;
        }
        if let Some(view) = self.view.as_mut() {
            match mouse.kind {
                MouseEventKind::ScrollDown => {
                    view.selected = (view.selected + 3).min(view.rows.len().saturating_sub(1))
                }
                MouseEventKind::ScrollUp => view.selected = view.selected.saturating_sub(3),
                MouseEventKind::Down(MouseButton::Left) => {
                    view.selected = (view.scroll + usize::from(mouse.row.saturating_sub(1)))
                        .min(view.rows.len().saturating_sub(1))
                }
                _ => {}
            }
            return;
        }
        let found = self
            .pane_areas(area)
            .into_iter()
            .find(|(_, rect)| rect.contains((mouse.column, mouse.row).into()));
        if let Some((index, rect)) = found {
            if index != self.active {
                // A focus click should never copy a word or activate a command in another pane.
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.focus_pane(index);
                }
                return;
            }
            if mouse.row <= rect.y {
                return;
            }
            mouse.column -= rect.x;
            mouse.row -= rect.y + 1;
            self.panes[index].handle_mouse(
                mouse,
                rect.width as usize,
                rect.height.saturating_sub(1) as usize,
            );
            self.process_action();
        }
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        loop {
            self.poll_job();
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            let areas = self.pane_areas(area);
            for (index, pane) in self.panes.iter_mut().enumerate() {
                let height = areas
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map(|(_, r)| r.height.saturating_sub(1))
                    .unwrap_or(size.height.saturating_sub(2));
                pane.tick(height);
            }
            if let Some(view) = self.view.as_mut() {
                let page = size.height.saturating_sub(2).max(1) as usize;
                if view.selected < view.scroll {
                    view.scroll = view.selected;
                }
                if view.selected >= view.scroll + page {
                    view.scroll = view.selected.saturating_sub(page - 1);
                }
            }
            terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()))?;
            if event::poll(Duration::from_millis(50))? {
                let height = self
                    .pane_areas(area)
                    .iter()
                    .find(|(i, _)| *i == self.active)
                    .map(|(_, r)| r.height.saturating_sub(1) as usize)
                    .unwrap_or(size.height as usize);
                match event::read()? {
                    Event::Key(key) => self.handle_key(
                        key,
                        if self.view.is_some() {
                            size.height as usize
                        } else {
                            height
                        },
                    ),
                    Event::Mouse(mouse) => self.handle_mouse(mouse, area),
                    _ => {}
                }
            }
            if self.panes.iter().any(|pane| pane.should_quit) {
                return Ok(());
            }
        }
    }
}

fn expand_path(input: &str) -> PathBuf {
    if let Some(relative) = input.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(relative);
    }
    PathBuf::from(input)
}

use crate::investigation::{self, EventRow, Redactor, Source};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

pub(crate) struct InvestigationView {
    sources: Vec<Source>,
    rows: Vec<EventRow>,
    selected: usize,
    scroll: usize,
    title: String,
    horizontal: usize,
    context: bool,
    anchor: Option<EventRow>,
}
enum JobOutcome {
    View(InvestigationView),
    Jump(EventRow),
    Export(usize, PathBuf),
    Report(usize, String, Vec<String>),
}
struct InvestigationJob {
    receiver: mpsc::Receiver<Result<JobOutcome, String>>,
    handle: Option<std::thread::JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    scanned: Arc<AtomicUsize>,
    total: usize,
    label: &'static str,
}
impl Drop for InvestigationJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Workspace {
    fn show_command(&mut self, initial: &str) {
        self.history_cursor = self.history.len();
        self.open_dialog = Some(OpenDialog {
            path: initial.into(),
            target: OpenTarget::Command,
            error: None,
        });
    }
    fn notify(&mut self, text: String, error: bool) {
        self.view = None;
        self.panes[self.active].modal = ActiveModal::Message {
            title: if error {
                "Investigation error"
            } else {
                "Investigation"
            }
            .into(),
            body: text,
            is_error: error,
        };
    }
    fn run_command_or_error(&mut self, command: &str) {
        if let Err(error) = self.execute_command(command) {
            self.notify(error, true);
        }
    }
    fn sources(&self) -> Result<Vec<Source>, String> {
        if self
            .panes
            .iter()
            .any(|app| app.reader.is_indexing.load(Ordering::Acquire))
        {
            return Err("Wait for file indexing to finish, then retry".into());
        }
        Ok(self.panes.iter().map(Source::capture).collect())
    }
    fn selected_event(&self) -> Result<EventRow, String> {
        if let Some(view) = &self.view {
            return view
                .rows
                .get(view.selected)
                .cloned()
                .ok_or("No event selected".into());
        }
        let app = &self.panes[self.active];
        let line = app
            .original_line_index_for_view(app.ui_state.selected_line)
            .ok_or("No event selected")?;
        let text = app
            .reader
            .read_line(line)
            .ok_or("Cannot read selected event")?;
        Ok(EventRow {
            source: self.active,
            line,
            timestamp_ms: crate::analytics::extract_timestamp(&text).map(|t| t.timestamp_millis()),
            matched: true,
            text,
        })
    }
    fn open_context(&mut self, anchor: EventRow, radius: usize) -> Result<(), String> {
        let sources = self.sources()?;
        let rows =
            investigation::context(&sources[anchor.source], anchor.source, anchor.line, radius)?;
        let selected = rows.iter().position(|r| r.line == anchor.line).unwrap_or(0);
        self.view = Some(InvestigationView {
            sources,
            rows,
            selected,
            scroll: selected.saturating_sub(8),
            title: format!("Context ±{radius} lines — * matches active filters, · context"),
            horizontal: 0,
            context: true,
            anchor: Some(anchor),
        });
        Ok(())
    }
    fn start_job(
        &mut self,
        sources: Vec<Source>,
        label: &'static str,
        work: impl FnOnce(Vec<Source>, Arc<AtomicBool>, Arc<AtomicUsize>) -> Result<JobOutcome, String>
        + Send
        + 'static,
    ) -> Result<(), String> {
        if self.job.is_some() {
            return Err("An investigation is running; Esc cancels it".into());
        }
        let total = sources.iter().map(|s| s.total).sum();
        let cancel = Arc::new(AtomicBool::new(false));
        let scanned = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = mpsc::channel();
        let c = cancel.clone();
        let p = scanned.clone();
        let handle = std::thread::spawn(move || {
            let result = work(sources, c, p);
            let _ = sender.send(result);
        });
        self.job = Some(InvestigationJob {
            receiver,
            handle: Some(handle),
            cancel,
            scanned,
            total,
            label,
        });
        Ok(())
    }
    fn cancel_job(&mut self) {
        self.job.take();
    }
    pub fn is_investigating(&self) -> bool {
        self.job.is_some()
    }
    pub fn investigation_rows(&self) -> Option<&[EventRow]> {
        self.view.as_ref().map(|v| v.rows.as_slice())
    }
    pub fn poll_job(&mut self) {
        let result = self
            .job
            .as_ref()
            .and_then(|job| match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Investigation worker stopped unexpectedly".into()))
                }
                Err(_) => None,
            });
        if let Some(result) = result {
            self.job.take();
            match result {
                Ok(JobOutcome::Report(pane, title, lines)) => {
                    self.view = None;
                    self.active = pane;
                    self.panes[pane].modal = ActiveModal::Report { title, lines, scroll: 0, horizontal: 0 };
                }
                Ok(JobOutcome::View(view)) => self.view = Some(view),
                Ok(JobOutcome::Jump(row)) => { if let Err(e) = self.reveal_event(&row, false) { self.notify(e, true); } }
                Ok(JobOutcome::Export(count, path)) => self.notify(format!("{count} events exported with sources, context and key-based redaction:\n{}",path.display()), false),
                Err(error) => self.notify(error, true),
            }
        }
    }
    /// All six investigation workflows are available through Ctrl+K and the Investigate menu.
    pub fn execute_command(&mut self, command: &str) -> Result<(), String> {
        if self.job.is_some() {
            return Err("An investigation is running; Esc cancels it".into());
        }
        let (verb, args) = command
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((command.trim(), ""));
        let args = args.trim();
        match verb {
            "histogram" => {
                let seconds = match args { "" | "hour" => 3600, "minute" => 60, "day" => 86400,
                    _ => return Err("Use histogram minute|hour|day".into()) };
                let pane = self.active;
                let source = self.sources()?.remove(pane);
                self.start_job(vec![source], "Counting filtered events", move |sources, cancel, scanned| {
                    let lines = crate::reports::frequency(&sources[0], seconds, &cancel, &scanned)?;
                    Ok(JobOutcome::Report(pane, "Occurrences over time — filtered snapshot".into(), lines))
                })?;
            }
            "compare" => {
                let pane = self.active;
                let mut sources = self.sources()?;
                let window = if args == "files" {
                    if sources.len() != 2 { return Err("Open two files, then compare files".into()); }
                    None
                } else {
                    let mut parts = args.split_whitespace();
                    let time = parts.next().ok_or("Use compare RFC3339_TIME [MINUTES] or compare files")?;
                    let pivot = chrono::DateTime::parse_from_rfc3339(time).map_err(|_| "Use an RFC3339 time with timezone")?.timestamp_millis();
                    let minutes = parts.next().unwrap_or("15").parse::<i64>().map_err(|_| "Minutes must be 1..10080")?;
                    if !(1..=10080).contains(&minutes) || parts.next().is_some() { return Err("Use compare TIME [MINUTES], 1..10080".into()); }
                    sources = vec![sources.remove(pane)];
                    Some((pivot, minutes * 60000))
                };
                self.start_job(sources, "Comparing patterns", move |sources, cancel, scanned| {
                    let lines = crate::reports::compare(&sources, window, &cancel, &scanned)?;
                    Ok(JobOutcome::Report(pane, "Before / after comparison".into(), lines))
                })?;
            }
            "context" => {
                let radius = if args.is_empty() { self.context_radius } else { args.parse::<usize>().map_err(|_| "Use context N (0..1000)")? };
                if radius > 1000 { return Err("Context is limited to 1000 lines on either side".into()); }
                let anchor = self.selected_event()?; self.open_context(anchor, radius)?; self.context_radius = radius;
            }
            "time" => {
                let pattern = format!("@time:{}", args.replace(' ', ""));
                // Validate before removing the previous time range.
                let _ = crate::filter::FilterRule::new(0, pattern.clone(), crate::filter::FilterKind::Include, false, false)?;
                let app = &mut self.panes[self.active];
                app.filter.rules.retain(|r| !r.pattern.starts_with("@time:"));
                app.filter.add_rule(pattern, crate::filter::FilterKind::Include, false, false, &app.reader)?;
                app.reset_view_after_filter_change(); self.view = None;
            }
            "untime" => {
                let app = &mut self.panes[self.active]; app.filter.rules.retain(|r|!r.pattern.starts_with("@time:"));
                app.filter.trigger_refilter(&app.reader); app.reset_view_after_filter_change(); self.view = None;
            }
            "around" => {
                let minutes: i64 = if args.is_empty() { 5 } else { args.parse().map_err(|_| "Use around MINUTES")? };
                if !(0..=525600).contains(&minutes) { return Err("Minutes must be between 0 and 525600".into()); }
                let row = self.selected_event()?; let time = row.timestamp_ms.ok_or("Selected event has no recognized timestamp")?;
                self.active = row.source;
                let start = chrono::DateTime::from_timestamp_millis(time-minutes*60000).ok_or("Time is out of range")?;
                let end = chrono::DateTime::from_timestamp_millis(time+minutes*60000).ok_or("Time is out of range")?;
                self.execute_command(&format!("time {}..{}",start.to_rfc3339(),end.to_rfc3339()))?;
            }
            "jump" => {
                let target = chrono::DateTime::parse_from_rfc3339(args).map_err(|_| "Use jump 2026-10-21T10:30:00Z (RFC3339 with timezone)")?.timestamp_millis();
                let active = self.active;
                let source = self.sources()?.swap_remove(active);
                self.start_job(vec![source],"Finding timestamp",move |sources,cancel,scanned| {
                    Ok(JobOutcome::Jump(investigation::find_timestamp(&sources[0],active,target,&cancel,&scanned)?))
                })?;
            }
            "merge" => {
                let sources = self.sources()?;
                self.start_job(sources, "Merging filtered sources", |sources,cancel,scanned| {
                    let rows = investigation::collect(&sources,&cancel,&scanned)?;
                    let unknown = rows.iter().filter(|r|r.timestamp_ms.is_none()).count();
                    Ok(JobOutcome::View(InvestigationView { sources, rows, selected:0, scroll:0, title:format!("Merged snapshot — active filters per file — {unknown} untimed events at end"), horizontal:0, context:false, anchor:None }))
                })?;
            }
            "trace" => {
                let _ = self.sources()?;
                let id = if args.is_empty() {
                    crate::analytics::TraceCorrelator::extract_identifiers(&self.selected_event()?.text).into_iter().find(|(key,_)|key != "ip").map(|(_,value)|value).ok_or("No correlation identifier on this event; use trace ID")?
                } else { args.to_string() };
                for app in &mut self.panes {
                    app.filter.add_rule(format!(r"(?:^|[^A-Za-z0-9_.-]){}(?:$|[^A-Za-z0-9_.-])",regex::escape(&id)),crate::filter::FilterKind::Include,true,true,&app.reader)?;
                    app.reset_view_after_filter_change();
                }
                self.view = None; self.execute_command("merge")?;
            }
            "save" => {
                if args.is_empty() { return Err("Use save PATH.json".into()); }
                let session = crate::session::Session { version:1, panes:self.panes.iter().map(crate::session::SavedPane::capture).collect::<Result<_,_>>()?, active:self.active, history:self.history.clone(), redaction_keys:self.redaction_keys.clone() };
                session.save(&expand_path(args))?; self.notify("Session saved (files, filters, columns, bookmarks and positions)".into(),false);
            }
            "load" => {
                if args.is_empty() { return Err("Use load PATH.json".into()); }
                let session = crate::session::Session::read(&expand_path(args))?;
                let mut panes = session.panes.iter().map(|pane|pane.restore()).collect::<Result<Vec<_>,_>>()?;
                let capture = self.panes[self.active].mouse_capture_enabled;
                for pane in &mut panes { pane.mouse_capture_enabled = capture; }
                self.cancel_job(); self.view = None; self.panes = panes; self.active = session.active;
                self.history = session.history; self.redaction_keys = session.redaction_keys;
            }
            "redact" => {
                if args.is_empty() { return Err("Use redact customer_id,email,... (adds to built-in secret keys); redact default resets extras".into()); }
                self.redaction_keys = if args == "default" { Vec::new() } else { args.split(',').map(|k|k.trim().to_owned()).filter(|k|!k.is_empty()).collect() };
                self.notify(format!("Built-in secret keys enabled; extra keys: {}",self.redaction_keys.join(", ")),false);
            }
            "export" => {
                if args.is_empty() { return Err("Use export PATH.jsonl; exports current investigation or filtered files with context and redaction".into()); }
                let path = expand_path(args); let radius = self.context_radius; let redactor = Redactor::new(&self.redaction_keys)?;
                let (sources, selected) = if let Some(view) = &self.view {
                    // A context view exports its anchor and surrounding context, not every neighbor as a match.
                    let rows = if view.context { view.anchor.clone().into_iter().collect() } else { view.rows.clone() };
                    (view.sources.clone(),Some(rows))
                } else { (self.sources()?,None) };
                self.start_job(sources,"Exporting incident",move |sources,cancel,scanned| {
                    let rows = match selected { Some(rows) => rows, None => investigation::collect(&sources,&cancel,&scanned)? };
                    let count = investigation::export_bundle(&path,&sources,&rows,radius,&redactor,&cancel)?;
                    Ok(JobOutcome::Export(count,path))
                })?;
            }
            "help" | "" => return Err("Commands: histogram minute|hour|day; compare TIME [MIN] | compare files; context N; time START..END; untime; jump TIME; around MIN; merge; trace [ID]; save PATH; load PATH; redact KEYS; export PATH. Times use RFC3339 with timezone. Esc cancels jobs/views; r refreshes merged view.".into()),
            _ => return Err(format!("Unknown command '{verb}'; use Ctrl+K then help")),
        }
        Ok(())
    }
    fn reveal_event(&mut self, row: &EventRow, clear_hidden: bool) -> Result<(), String> {
        let app = self
            .panes
            .get_mut(row.source)
            .ok_or("Event source is no longer open")?;
        let display = if app.filter.has_active_filters() {
            app.filter
                .filtered_indices
                .read()
                .iter()
                .position(|idx| *idx == row.line)
        } else {
            Some(row.line)
        };
        let index = match display {
            Some(index) => index,
            None if clear_hidden => {
                app.filter.clear_rules(&app.reader);
                app.filter.sync_field_filter(
                    &app.format.available_fields,
                    &app.format.available_fields,
                    false,
                    &app.reader,
                );
                row.line
            }
            None => {
                return Err(
                    "Event is hidden or filtering is still running; open context to inspect it"
                        .into(),
                );
            }
        };
        self.active = row.source;
        let total = app.filter.filtered_count(&app.reader);
        let index = crate::ui::source_display_index(index, total, app.ui_state.reverse_order);
        app.ui_state.selected_line = index;
        app.ui_state.scroll_row = index.saturating_sub(5);
        app.ui_state.tail_following = false;
        self.view = None;
        Ok(())
    }
    fn view_key(&mut self, key: KeyEvent, height: usize) {
        match key.code {
            KeyCode::Esc => {
                self.view = None;
                return;
            }
            KeyCode::Char('Q') => {
                self.panes[self.active].should_quit = true;
                return;
            }
            KeyCode::Char('r') => {
                self.run_command_or_error("merge");
                return;
            }
            KeyCode::Char('c') => {
                self.run_command_or_error(&format!("context {}", self.context_radius));
                return;
            }
            KeyCode::Enter => {
                if self.view.as_ref().is_some_and(|view| {
                    view.sources.iter().any(|source| source.validate().is_err())
                }) {
                    self.view = None;
                    self.notify(
                        "Sources changed; rebuild the investigation before jumping".into(),
                        true,
                    );
                    return;
                }
                if let Ok(row) = self.selected_event()
                    && let Err(err) = self.reveal_event(&row, true)
                {
                    self.notify(err, true);
                }
                return;
            }
            KeyCode::Char('n') | KeyCode::Char('p')
                if self.view.as_ref().is_some_and(|v| v.context) =>
            {
                let anchor = self.view.as_ref().and_then(|v| v.anchor.clone()).unwrap();
                let app = &self.panes[anchor.source];
                let next = if app.filter.has_active_filters() {
                    let indices = app.filter.filtered_indices.read();
                    if key.code == KeyCode::Char('n') {
                        indices.iter().find(|i| **i > anchor.line).copied()
                    } else {
                        indices.iter().rev().find(|i| **i < anchor.line).copied()
                    }
                } else if key.code == KeyCode::Char('n') {
                    (anchor.line + 1 < app.reader.line_count()).then_some(anchor.line + 1)
                } else {
                    anchor.line.checked_sub(1)
                };
                if let Some(line) = next {
                    let text = app.reader.read_line(line).unwrap_or_default();
                    let row = EventRow {
                        line,
                        text,
                        ..anchor
                    };
                    if let Err(err) = self.open_context(row, self.context_radius) {
                        self.notify(err, true);
                    }
                }
                return;
            }
            _ => {}
        }
        let view = self.view.as_mut().unwrap();
        let page = height.saturating_sub(3).max(1);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                view.selected = (view.selected + 1).min(view.rows.len().saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') => view.selected = view.selected.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => {
                view.selected = (view.selected + page).min(view.rows.len().saturating_sub(1))
            }
            KeyCode::PageUp => view.selected = view.selected.saturating_sub(page),
            KeyCode::Left => view.horizontal = view.horizontal.saturating_sub(8),
            KeyCode::Right => view.horizontal = view.horizontal.saturating_add(8),
            KeyCode::Home => view.selected = 0,
            KeyCode::End => view.selected = view.rows.len().saturating_sub(1),
            _ => {}
        }
        if view.selected < view.scroll {
            view.scroll = view.selected;
        }
        if view.selected >= view.scroll + page {
            view.scroll = view.selected.saturating_sub(page - 1);
        }
    }
    fn render_investigation(&self, view: &InvestigationView, area: Rect, buf: &mut Buffer) {
        use ratatui::text::{Line, Span};
        Clear.render(area, buf);
        if area.height == 0 {
            return;
        }
        Paragraph::new(format!(" {} | {} events", view.title, view.rows.len()))
            .style(Style::default().fg(Color::Black).bg(Color::Cyan))
            .render(Rect::new(area.x, area.y, area.width, 1), buf);
        let height = area.height.saturating_sub(2) as usize;
        for (offset, row) in view.rows.iter().skip(view.scroll).take(height).enumerate() {
            let filename = view
                .sources
                .get(row.source)
                .and_then(|s| s.path.file_name())
                .unwrap_or_default()
                .to_string_lossy();
            let selected = view.scroll + offset == view.selected;
            let style = Style::default()
                .fg(if row.matched {
                    Color::White
                } else {
                    Color::DarkGray
                })
                .bg(if selected {
                    Color::DarkGray
                } else {
                    Color::Black
                });
            let prefix = format!(
                "{} {} [{}:{}] ",
                if selected { ">" } else { " " },
                if row.matched { "*" } else { "·" },
                filename,
                row.line + 1
            );
            Paragraph::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::raw(crate::ui::skip_columns(&row.text, view.horizontal)),
            ]))
            .style(style)
            .render(
                Rect::new(area.x, area.y + 1 + offset as u16, area.width, 1),
                buf,
            );
        }
        if area.height > 1 {
            Paragraph::new(" Esc: panes | ↑↓/PgUp/PgDn: browse | n/p: next/prev match | c: context | Enter: reveal (clears filters if hidden) | r: refresh merge | Ctrl+K: commands ").style(Style::default().fg(Color::Yellow))
                .render(Rect::new(area.x,area.bottom()-1,area.width,1),buf);
        }
    }
}
