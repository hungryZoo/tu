//! Pure render functions — they read `AppState`, write geometry back
//! into `state.geom`, but do *not* spawn subprocesses or mutate
//! anything else.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::state::{
    AppState, ButtonId, ConfFocus, DeleteFocus, Focus, HitTarget, Screen, StatusKind, BUTTON_ORDER,
};
use crate::theme::{self, ButtonVariant, ButtonVisual};

pub fn render(f: &mut Frame, state: &mut AppState) {
    let area = f.area();

    let outer = Block::new()
        .borders(Borders::ALL)
        .border_style(theme::outer_border_style(state.inside_tmux))
        .title(Span::styled(
            format!(
                " tu — {} tmux ",
                if state.inside_tmux { "in" } else { "outside" }
            ),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let body = inner.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // section label
            Constraint::Min(4),    // session list
            Constraint::Length(1), // spacer
            Constraint::Length(3), // buttons
            Constraint::Length(1), // hint
            Constraint::Length(1), // status
        ])
        .split(body);

    render_section_label(f, chunks[0]);
    render_session_list(f, chunks[1], state);
    render_buttons(f, chunks[3], state);
    render_hint(f, chunks[4]);
    render_status(f, chunks[5], state);

    // Snapshot the modal data before passing &mut state to render
    // helpers — keeps the borrow checker happy.
    let modal = match &state.screen {
        Screen::Main => None,
        Screen::ConfirmDelete { name, focus } => Some(ModalSnapshot::Delete(name.clone(), *focus)),
        Screen::ConfSetup { directives, focus } => {
            let lines: Vec<String> = directives.iter().map(|d| d.line()).collect();
            Some(ModalSnapshot::Conf(lines, *focus))
        }
    };

    if let Some(snap) = modal {
        // Clear geometry from the main view so hit-testing while a
        // modal is up doesn't accidentally land on a button beneath.
        // (`hit_test` already short-circuits on non-Main screens, but
        // belt-and-braces.)
        match snap {
            ModalSnapshot::Delete(name, focus) => {
                render_confirm_delete(f, area, state, &name, focus);
            }
            ModalSnapshot::Conf(lines, focus) => {
                render_conf_setup(f, area, state, &lines, focus);
            }
        }
    } else {
        state.geom.modal_primary = None;
        state.geom.modal_secondary = None;
    }
}

enum ModalSnapshot {
    Delete(String, DeleteFocus),
    Conf(Vec<String>, ConfFocus),
}

// -------------------------------------------------------- sections

fn render_section_label(f: &mut Frame, area: Rect) {
    let label = Paragraph::new(Span::styled(
        "Sessions",
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(label, area);
}

fn render_session_list(f: &mut Frame, area: Rect, state: &mut AppState) {
    let list_focused = matches!(state.focus, Focus::List);

    let border_style = if list_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);
    state.geom.list_inner = Some(inner);

    state.ensure_list_offset(inner.height);

    if state.sessions.is_empty() {
        let empty = Paragraph::new("No tmux sessions yet — press n to create one.")
            .style(theme::hint_style())
            .alignment(Alignment::Center);
        f.render_widget(empty, inner);
        return;
    }

    let max_name = state
        .sessions
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(8)
        .max(8);

    let visible_count = inner.height as usize;
    for (visible_i, (abs_i, session)) in state
        .sessions
        .iter()
        .enumerate()
        .skip(state.list_offset)
        .take(visible_count)
        .enumerate()
    {
        let row_y = inner.y + visible_i as u16;
        let row_rect = Rect::new(inner.x, row_y, inner.width, 1);

        let is_selected = abs_i == state.selected;
        let is_hovered = matches!(state.hover, Some(HitTarget::ListRow(r)) if r == abs_i);

        let row_style = if is_selected {
            theme::list_selected_style(list_focused)
        } else if is_hovered {
            theme::list_hover_style()
        } else {
            Style::default()
        };

        let prefix = if is_selected { "▶ " } else { "  " };
        let spans = if is_selected || is_hovered {
            let attached = if session.attached { " · attached" } else { "" };
            vec![Span::raw(format!(
                "{}{:width$}  {:>3}w{}",
                prefix,
                session.name,
                session.windows,
                attached,
                width = max_name
            ))]
        } else {
            let mut spans = vec![
                Span::raw(prefix.to_string()),
                Span::raw(format!("{:width$}", session.name, width = max_name)),
                Span::raw("  "),
                Span::styled(
                    format!("{:>3}w", session.windows),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            if session.attached {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    "● attached",
                    theme::list_attached_marker_style(),
                ));
            }
            spans
        };

        let para = Paragraph::new(Line::from(spans)).style(row_style);
        f.render_widget(para, row_rect);
    }
}

// --------------------------------------------------------- buttons

fn render_buttons(f: &mut Frame, area: Rect, state: &mut AppState) {
    let spacer: u16 = 2;
    let widths: Vec<u16> = BUTTON_ORDER
        .iter()
        .map(|b| (b.label().chars().count() as u16) + 4) // 2 borders + 2 padding
        .collect();

    let total: u16 = widths.iter().sum::<u16>() + spacer * (BUTTON_ORDER.len() as u16 - 1);
    let start_x = if total <= area.width {
        area.x + (area.width - total) / 2
    } else {
        area.x
    };

    let mut x = start_x;
    for (i, button) in BUTTON_ORDER.iter().enumerate() {
        let w = widths[i].min(area.width.saturating_sub(x - area.x));
        if w == 0 {
            state.geom.buttons[i] = None;
            continue;
        }
        let rect = Rect::new(x, area.y, w, area.height);
        state.geom.buttons[i] = Some(rect);
        render_main_button(f, rect, state, *button);
        x += widths[i] + spacer;
    }
}

fn render_main_button(f: &mut Frame, area: Rect, state: &AppState, button: ButtonId) {
    let visual = button_visual(state, button);
    render_styled_button(f, area, button.label(), button.variant(), visual);
}

fn button_visual(state: &AppState, button: ButtonId) -> ButtonVisual {
    if !button_enabled(state, button) {
        return ButtonVisual::Disabled;
    }
    if matches!(state.pressed, Some(HitTarget::Button(b)) if b == button) {
        return ButtonVisual::Pressed;
    }
    if matches!(state.hover, Some(HitTarget::Button(b)) if b == button) {
        return ButtonVisual::Hover;
    }
    if matches!(state.focus, Focus::Button(b) if b == button) {
        return ButtonVisual::Focus;
    }
    ButtonVisual::Idle
}

fn button_enabled(state: &AppState, button: ButtonId) -> bool {
    match button {
        ButtonId::New | ButtonId::Quit => true,
        ButtonId::Attach | ButtonId::Delete => state.has_sessions(),
        ButtonId::Detach => state.inside_tmux,
    }
}

fn render_styled_button(
    f: &mut Frame,
    area: Rect,
    label: &str,
    variant: ButtonVariant,
    visual: ButtonVisual,
) {
    let style = theme::button_style(variant, visual);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(style.border)
        .style(style.label);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let para = Paragraph::new(label)
        .style(style.label)
        .alignment(Alignment::Center);
    f.render_widget(para, inner);
}

// ---------------------------------------------------- hint + status

fn render_hint(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("n", bold()),
        Span::raw(" New  "),
        Span::styled("a", bold()),
        Span::raw(" Attach  "),
        Span::styled("d", bold()),
        Span::raw(" Detach  "),
        Span::styled("q", bold()),
        Span::raw(" Quit  "),
        Span::styled("del", bold()),
        Span::raw(" Delete  "),
        Span::styled("↑↓", bold()),
        Span::raw(" Nav  "),
        Span::styled("Tab", bold()),
        Span::raw(" Focus  "),
        Span::styled("Enter", bold()),
        Span::raw(" Attach"),
    ]);
    let p = Paragraph::new(line).style(theme::hint_style());
    f.render_widget(p, area);
}

fn bold() -> Style {
    Style::default()
        .fg(Color::Gray)
        .add_modifier(Modifier::BOLD)
}

fn render_status(f: &mut Frame, area: Rect, state: &AppState) {
    let style = match state.status_kind {
        StatusKind::Info => theme::status_info_style(),
        StatusKind::Error => theme::status_error_style(),
    };
    let p = Paragraph::new(state.status_message.clone()).style(style);
    f.render_widget(p, area);
}

// ---------------------------------------------------------- modals

fn render_confirm_delete(
    f: &mut Frame,
    area: Rect,
    state: &mut AppState,
    name: &str,
    focus: DeleteFocus,
) {
    let title = format!(" Delete session '{}' ", name);
    let body = vec![
        Line::from(""),
        Line::from(format!("Really delete session '{}'?", name)),
        Line::from(Span::styled(
            "This cannot be undone.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    render_modal(
        f,
        area,
        state,
        &title,
        body,
        ("Back", ButtonVariant::Default),
        ("Delete", ButtonVariant::Danger),
        matches!(focus, DeleteFocus::Cancel),
        matches!(focus, DeleteFocus::Confirm),
    );
}

fn render_conf_setup(
    f: &mut Frame,
    area: Rect,
    state: &mut AppState,
    directive_lines: &[String],
    focus: ConfFocus,
) {
    let title = " ~/.tmux.conf ".to_string();
    let mut body = vec![
        Line::from(""),
        Line::from("Add the following to your ~/.tmux.conf?"),
        Line::from(""),
    ];
    for line in directive_lines {
        body.push(Line::from(Span::styled(
            format!("  {line}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    }
    body.push(Line::from(""));
    body.push(Line::from(Span::styled(
        "We'll patch the live tmux server too.",
        Style::default().fg(Color::DarkGray),
    )));

    // "Yes" is the action button (Danger variant for emphasis would
    // be wrong here — it's a helpful change), so we render it with
    // the default variant but mark it Focused/Pressed normally.
    render_modal(
        f,
        area,
        state,
        &title,
        body,
        ("Yes, add (y)", ButtonVariant::Default),
        ("Later (n)", ButtonVariant::Grey),
        // "Yes" is the primary (the user opened tu, presumably they
        // want a saner default) — so it is the focus default.
        matches!(focus, ConfFocus::Yes),
        matches!(focus, ConfFocus::Later),
    );
}

#[allow(clippy::too_many_arguments)]
fn render_modal(
    f: &mut Frame,
    area: Rect,
    state: &mut AppState,
    title: &str,
    body: Vec<Line>,
    primary_btn: (&str, ButtonVariant),
    secondary_btn: (&str, ButtonVariant),
    primary_focused: bool,
    secondary_focused: bool,
) {
    let widest_line = body.iter().map(|l| l.width() as u16).max().unwrap_or(40);
    let width = widest_line.max(48).min(area.width.saturating_sub(4));
    let height = (body.len() as u16 + 6).min(area.height.saturating_sub(2));
    let modal_area = centered_rect(width + 4, height, area);

    f.render_widget(Clear, modal_area);

    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(theme::modal_border_style())
        .title(Span::styled(
            title.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(2), Constraint::Length(3)])
        .split(inner);

    let body_para = Paragraph::new(body).alignment(Alignment::Center);
    f.render_widget(body_para, layout[0]);

    // -------- buttons row inside the modal --------
    let pw = (primary_btn.0.chars().count() as u16) + 4;
    let sw = (secondary_btn.0.chars().count() as u16) + 4;
    let spacer: u16 = 3;
    let total = pw + sw + spacer;
    let row = layout[1];
    let start_x = row.x + row.width.saturating_sub(total) / 2;

    let primary_rect = Rect::new(start_x, row.y, pw, row.height);
    let secondary_rect = Rect::new(start_x + pw + spacer, row.y, sw, row.height);
    state.geom.modal_primary = Some(primary_rect);
    state.geom.modal_secondary = Some(secondary_rect);

    let primary_visual = modal_button_visual(state, HitTarget::ModalPrimary, primary_focused);
    let secondary_visual = modal_button_visual(state, HitTarget::ModalSecondary, secondary_focused);

    render_styled_button(
        f,
        primary_rect,
        primary_btn.0,
        primary_btn.1,
        primary_visual,
    );
    render_styled_button(
        f,
        secondary_rect,
        secondary_btn.0,
        secondary_btn.1,
        secondary_visual,
    );
}

fn modal_button_visual(state: &AppState, target: HitTarget, focused: bool) -> ButtonVisual {
    if matches!(state.pressed, Some(t) if t == target) {
        ButtonVisual::Pressed
    } else if matches!(state.hover, Some(t) if t == target) {
        ButtonVisual::Hover
    } else if focused {
        ButtonVisual::Focus
    } else {
        ButtonVisual::Idle
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
