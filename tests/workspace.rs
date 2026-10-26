use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use flik::{app::App, ui::menu::MenuAction, workspace::Workspace};
use ratatui::{buffer::Buffer, layout::Rect};
use std::{
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}
fn wait(mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn open_dialog(workspace: &mut Workspace, shortcut: char, path: &Path) {
    workspace.handle_key(ctrl(KeyCode::Char(shortcut)), 30);
    for ch in path.to_str().unwrap().chars() {
        workspace.handle_key(key(KeyCode::Char(ch)), 30);
    }
    workspace.handle_key(key(KeyCode::Enter), 30);
}
fn rendered(workspace: &Workspace, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    workspace.render(area, &mut buffer);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn two_sessions_filter_and_navigate_independently() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left.log");
    let right = dir.path().join("right.log");
    std::fs::write(&left, "ERROR left failure\nINFO left normal\n").unwrap();
    std::fs::write(&right, "ERROR right failure\nDEBUG right detail\n").unwrap();
    let mut ws = Workspace::new(App::new(left.clone()).unwrap());
    open_dialog(&mut ws, 'n', &right);
    assert_eq!(ws.panes.len(), 2);
    assert_eq!(ws.active, 1);
    for (expected_pane, pattern) in [(1, "DEBUG"), (0, "ERROR")] {
        assert_eq!(ws.active, expected_pane);
        ws.handle_key(key(KeyCode::Char('3')), 30);
        for ch in pattern.chars() {
            ws.handle_key(key(KeyCode::Char(ch)), 30);
        }
        ws.handle_key(key(KeyCode::Enter), 30);
        wait(|| {
            !ws.panes[expected_pane]
                .filter
                .is_filtering
                .load(Ordering::Acquire)
        });
        assert_eq!(
            ws.panes[expected_pane]
                .filter
                .filtered_count(&ws.panes[expected_pane].reader),
            1
        );
        ws.handle_key(
            ctrl(if expected_pane == 1 {
                KeyCode::Left
            } else {
                KeyCode::Right
            }),
            30,
        );
    }
    assert_eq!(ws.panes[0].filter.rules[0].pattern, "ERROR");
    assert_eq!(ws.panes[1].filter.rules[0].pattern, "DEBUG");
    let screen = rendered(&ws, 200, 40);
    assert!(screen.contains("left failure"), "{screen}");
    assert!(screen.contains("right detail"), "{screen}");
    assert!(!screen.contains("left normal"));
    assert!(!screen.contains("right failure"));
    // Clear applies only to the focused pane.
    ws.handle_key(key(KeyCode::Char('X')), 30);
    assert!(!ws.panes[1].filter.has_active_filters());
    assert!(ws.panes[0].filter.has_active_filters());
}
#[test]
fn failed_open_cancel_replace_and_close_preserve_other_session() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left.log");
    let right = dir.path().join("right.log");
    std::fs::write(&left, "left\n").unwrap();
    std::fs::write(&right, "right\n").unwrap();
    let mut ws = Workspace::new(App::new(left.clone()).unwrap());
    open_dialog(&mut ws, 'n', &dir.path().join("missing.log"));
    assert_eq!(ws.panes.len(), 1);
    assert!(rendered(&ws, 120, 30).contains("No such file"));
    ws.handle_key(key(KeyCode::Esc), 30);
    assert_eq!(ws.panes[0].reader.path, left);
    assert!(ws.open_other(dir.path()).is_err());
    ws.open_other(&right).unwrap();
    ws.panes[0].ui_state.scroll_col = 7;
    open_dialog(&mut ws, 'o', &left);
    assert_eq!(ws.panes[0].ui_state.scroll_col, 7);
    assert_eq!(ws.panes[1].reader.path, left);
    ws.handle_key(ctrl(KeyCode::Char('w')), 30);
    assert_eq!(ws.panes.len(), 1);
    assert_eq!(ws.panes[0].ui_state.scroll_col, 7);
    ws.close_active();
    assert_eq!(ws.panes.len(), 1);
    assert!(!ws.panes[0].should_quit);
}
#[test]
fn mouse_coordinates_focus_and_resizing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("events.log");
    std::fs::write(&file, "first\nsecond\nthird\n").unwrap();
    let mut ws = Workspace::new(App::new(file.clone()).unwrap());
    ws.open_other(&file).unwrap();
    ws.active = 0;
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 110,
        row: 4,
        modifiers: KeyModifiers::NONE,
    };
    ws.handle_mouse(click, Rect::new(0, 0, 200, 40));
    assert_eq!(ws.active, 1);
    assert_eq!(ws.panes[1].ui_state.selected_line, 0); // first click only changes focus
    ws.handle_mouse(click, Rect::new(0, 0, 200, 40));
    assert_eq!(ws.panes[1].ui_state.selected_line, 1);
    assert_eq!(ws.panes[0].ui_state.selected_line, 0);
    for (width, height) in [(200, 40), (80, 40), (40, 12), (12, 6), (1, 1), (0, 0)] {
        rendered(&ws, width, height);
    }
    // Menus request a workspace action through the same path as shortcuts.
    ws.panes[1].execute_menu_action(MenuAction::ClosePane);
    ws.handle_key(key(KeyCode::Null), 30);
    assert_eq!(ws.panes.len(), 1);
}

#[test]
fn focus_shortcuts_work_while_editing_filters_and_keep_each_draft() {
    use flik::ui::modals::ActiveModal;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.log");
    std::fs::write(&path, "first\nsecond\n").unwrap();
    let mut ws = Workspace::new(App::new(path.clone()).unwrap());
    ws.open_other(&path).unwrap();
    ws.active = 0;
    ws.handle_key(key(KeyCode::Char('3')), 30);
    for c in "left draft".chars() {
        ws.handle_key(key(KeyCode::Char(c)), 30);
    }
    ws.handle_key(ctrl(KeyCode::Right), 30);
    assert_eq!(ws.active, 1);
    ws.handle_key(key(KeyCode::Char('3')), 30);
    for c in "right draft".chars() {
        ws.handle_key(key(KeyCode::Char(c)), 30);
    }
    ws.handle_key(ctrl(KeyCode::Left), 30);
    assert_eq!(ws.active, 0);
    assert!(
        matches!(&ws.panes[0].modal, ActiveModal::AddFilter {pattern,..} if pattern == "left draft")
    );
    assert!(
        matches!(&ws.panes[1].modal, ActiveModal::AddFilter {pattern,..} if pattern == "right draft")
    );
    ws.handle_key(ctrl(KeyCode::Left), 30);
    assert_eq!(ws.active, 0);
    ws.handle_key(ctrl(KeyCode::Char(']')), 30);
    assert_eq!(ws.active, 1);
    ws.handle_key(key(KeyCode::F(12)), 30);
    assert_eq!(ws.active, 0);
    ws.handle_key(ctrl(KeyCode::Char('5')), 30); // Legacy Ctrl+] decoded by crossterm.
    assert_eq!(ws.active, 1);
    assert!(ws.panes[0].reader.is_tail_enabled());
}

#[test]
fn clicks_focus_other_pane_even_with_menu_or_filter_open() {
    use flik::ui::modals::ActiveModal;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.log");
    std::fs::write(&path, "first\nsecond\n").unwrap();
    for (width, height, x, y) in [(200, 40, 110, 4), (80, 40, 10, 25), (200, 40, 110, 0)] {
        let mut ws = Workspace::new(App::new(path.clone()).unwrap());
        ws.open_other(&path).unwrap();
        ws.active = 0;
        ws.panes[0].menu.toggle();
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        };
        ws.handle_mouse(click, Rect::new(0, 0, width, height));
        assert_eq!(ws.active, 1);
        assert!(!ws.panes[0].menu.is_open);
        // Focus clicks never invoke another pane's toolbar or select a row.
        assert_eq!(ws.panes[1].ui_state.selected_line, 0);
        ws.handle_key(key(KeyCode::Char('3')), 30);
        ws.handle_key(key(KeyCode::Char('z')), 30);
        ws.handle_mouse(
            MouseEvent {
                column: 10,
                row: 0,
                ..click
            },
            Rect::new(0, 0, width, height),
        );
        assert_eq!(ws.active, 0);
        assert!(matches!(&ws.panes[1].modal,ActiveModal::AddFilter{pattern,..} if pattern == "z"));
    }
}
