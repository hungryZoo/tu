//! Pure render functions — they read `AppState`, write geometry back
//! into `state.geom`, but do *not* spawn subprocesses or mutate
//! anything else.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::state::{
    AppState, ButtonId, ConfFocus, DeleteFocus, Focus, HitTarget, Screen, StatusKind, BUTTON_ORDER,
};
use crate::theme::{self, palette, ButtonVisual};

pub fn render(f: &mut Frame, state: &mut AppState) {
    let area = f.area();

    // Window — outer border + opaque Base background so the colour
    // story owns the whole frame regardless of the user's terminal
    // theme.
    let outer = Block::new()
        .borders(Borders::ALL)
        .border_style(theme::outer_border_style(state.inside_tmux))
        .style(theme::window_bg())
        .title(build_window_title())
        .title_top(close_chip(state).right_aligned());
    let inner = outer.inner(area);
    f.render_widget(outer, area);
    // Right-aligned titles end one cell before the corner.
    let chip_w = CLOSE_CHIP.chars().count() as u16;
    state.geom.close = if area.width > chip_w + 2 {
        Some(Rect::new(area.right() - 1 - chip_w, area.y, chip_w, 1))
    } else {
        None
    };

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
            Constraint::Length(1), // status
        ])
        .split(body);

    render_section_label(f, chunks[0]);
    render_session_list(f, chunks[1], state);
    render_buttons(f, chunks[3], state);
    render_status(f, chunks[4], state);

    // Snapshot modal data before passing `&mut state` to render
    // helpers — keeps the borrow checker happy.
    let modal = match &state.screen {
        Screen::Main => None,
        Screen::ConfirmDelete { name, focus } => Some(ModalSnapshot::Delete(name.clone(), *focus)),
        Screen::Rename { original, input } => {
            Some(ModalSnapshot::Rename(original.clone(), input.clone()))
        }
        Screen::Update { plan } => Some(ModalSnapshot::Update(plan.clone())),
        Screen::ConfSetup { items, focus } => {
            let lines: Vec<String> = items.iter().flat_map(|i| i.lines()).collect();
            Some(ModalSnapshot::Conf(lines, *focus))
        }
        Screen::RestartNotice { message } => Some(ModalSnapshot::Notice(message.clone())),
    };

    if let Some(snap) = modal {
        match snap {
            ModalSnapshot::Delete(name, focus) => {
                render_confirm_delete(f, area, state, &name, focus);
            }
            ModalSnapshot::Rename(original, input) => {
                render_rename(f, area, state, &original, &input);
            }
            ModalSnapshot::Update(plan) => render_update(f, area, state, &plan),
            ModalSnapshot::Conf(lines, focus) => {
                render_conf_setup(f, area, state, &lines, focus);
            }
            ModalSnapshot::Notice(message) => {
                render_restart_notice(f, area, state, &message);
            }
        }
    } else {
        state.geom.modal_primary = None;
        state.geom.modal_secondary = None;
    }
}

enum ModalSnapshot {
    Delete(String, DeleteFocus),
    Rename(String, String),
    Update(crate::update::UpdatePlan),
    Conf(Vec<String>, ConfFocus),
    Notice(String),
}

// -------------------------------------------------------- window title

fn build_window_title() -> Line<'static> {
    Line::from(Span::styled(" tu ", theme::title_brand_style()))
}

/// The close "button": a filled chip in the top border, mirroring the
/// ` tu ` brand chip on the left. Grey block with a red `x` at rest,
/// lighter when hovered / focused, solid red while pressed.
const CLOSE_CHIP: &str = " x ";

fn close_chip(state: &AppState) -> Line<'static> {
    let visual = button_visual(state, ButtonId::Close);
    Line::from(Span::styled(CLOSE_CHIP, theme::close_chip_style(visual)))
}

// -------------------------------------------------------- section

fn render_section_label(f: &mut Frame, area: Rect) {
    let label = Paragraph::new(Line::from(vec![
        Span::styled("◆ ", theme::section_marker_style()),
        Span::styled("Sessions", theme::section_label_style()),
    ]))
    .style(theme::window_bg());
    f.render_widget(label, area);
}

// -------------------------------------------------------- session list

fn render_session_list(f: &mut Frame, area: Rect, state: &mut AppState) {
    // Keyboard focus lives on the button row; the list always shows
    // its selection in the strong style, since ↑/↓ drive it from
    // anywhere.
    let list_focused = true;

    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(theme::list_block_border_style(list_focused))
        .style(theme::window_bg());
    let inner = block.inner(area);
    f.render_widget(block, area);
    state.geom.list_inner = Some(inner);

    state.ensure_list_offset(inner.height);

    if state.sessions.is_empty() {
        let empty = Paragraph::new("No tmux sessions yet — press n to create one.")
            .style(theme::list_empty_style())
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
            theme::list_row_default_style()
        };

        let prefix = if is_selected { "▶ " } else { "  " };
        let spans: Vec<Span<'static>> = if is_selected || is_hovered {
            // Uniform row style — keeps the highlight bar clean.
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
            let mut s = vec![
                Span::styled(prefix.to_string(), theme::list_row_default_style()),
                Span::styled(
                    format!("{:width$}", session.name, width = max_name),
                    theme::list_row_default_style(),
                ),
                Span::styled("  ", theme::list_row_default_style()),
                Span::styled(
                    format!("{:>3}w", session.windows),
                    theme::list_row_dim_style(),
                ),
            ];
            if session.attached {
                s.push(Span::styled("  ", theme::list_row_default_style()));
                s.push(Span::styled(
                    "● attached",
                    theme::list_attached_marker_style(),
                ));
            }
            s
        };

        let para = Paragraph::new(Line::from(spans)).style(row_style);
        f.render_widget(para, row_rect);
    }
}

// --------------------------------------------------------- buttons

/// How much of each button we can afford at the current width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTier {
    /// `New (n)` — key inside parentheses, highlighted.
    Full,
    /// `New` with its initial highlighted; `Delete Bksp`.
    Word,
    /// Bordered box holding only the key: `N`, `Esc`.
    KeyBoxed,
    /// No border; the key alone in the button's accent colour.
    KeyBare,
}

const BUTTON_SPACER: u16 = 1;

/// Cell widths of the six buttons at a tier (borders / padding included).
fn tier_widths(tier: ButtonTier) -> Vec<u16> {
    BUTTON_ORDER
        .iter()
        .map(|b| {
            let text = match tier {
                ButtonTier::Full => {
                    let (a, k, c) = b.full_parts();
                    a.chars().count() + k.chars().count() + c.chars().count()
                }
                ButtonTier::Word => b.label().chars().count(),
                ButtonTier::KeyBoxed | ButtonTier::KeyBare => b.key_only().chars().count(),
            } as u16;
            match tier {
                ButtonTier::KeyBare => text + 2, // one space each side, no border
                _ => text + 4,                   // border + one space each side
            }
        })
        .collect()
}

fn tier_total(tier: ButtonTier) -> u16 {
    tier_widths(tier).iter().sum::<u16>() + BUTTON_SPACER * (BUTTON_ORDER.len() as u16 - 1)
}

/// Widest tier that fits; KeyBare when nothing does.
pub fn button_tier(width: u16) -> ButtonTier {
    [
        ButtonTier::Full,
        ButtonTier::Word,
        ButtonTier::KeyBoxed,
        ButtonTier::KeyBare,
    ]
    .into_iter()
    .find(|t| tier_total(*t) <= width)
    .unwrap_or(ButtonTier::KeyBare)
}

fn render_buttons(f: &mut Frame, area: Rect, state: &mut AppState) {
    let tier = button_tier(area.width);
    let widths = tier_widths(tier);
    let total = tier_total(tier);
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
        let visual = button_visual(state, *button);
        let style = theme::button_style(button.accent(), visual);
        let key_style = theme::button_key_style(style.label);
        match tier {
            ButtonTier::Full | ButtonTier::Word => {
                let rect = Rect::new(x, area.y, w, area.height);
                state.geom.buttons[i] = Some(rect);
                let (before, key, after) = if tier == ButtonTier::Full {
                    button.full_parts()
                } else {
                    button.word_parts()
                };
                let line = Line::from(vec![
                    Span::styled(before, style.label),
                    Span::styled(key, key_style),
                    Span::styled(after, style.label),
                ]);
                render_button_frame(f, rect, line, &style);
            }
            ButtonTier::KeyBoxed => {
                let rect = Rect::new(x, area.y, w, area.height);
                state.geom.buttons[i] = Some(rect);
                let line = Line::from(Span::styled(button.key_only(), key_style));
                render_button_frame(f, rect, line, &style);
            }
            ButtonTier::KeyBare => {
                // One row, vertically centred in the 3-row button band.
                // The key wears the accent the border would have had;
                // hover / focus / pressed keep their fill so the cursor
                // position still reads as a grey (or accent) block.
                let rect = Rect::new(x, area.y + area.height / 2, w, 1);
                state.geom.buttons[i] = Some(rect);
                let text_style = theme::bare_button_style(button.accent(), visual);
                let p = Paragraph::new(button.key_only())
                    .style(text_style)
                    .alignment(Alignment::Center);
                f.render_widget(p, rect);
            }
        }
        x += widths[i] + BUTTON_SPACER;
    }
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
        ButtonId::New | ButtonId::Close => true,
        ButtonId::Attach | ButtonId::Rename | ButtonId::Delete => state.has_sessions(),
        ButtonId::Detach => state.inside_tmux,
    }
}

/// Modal button: plain centred label on the Crust modal background.
fn render_modal_button(
    f: &mut Frame,
    area: Rect,
    label: &str,
    accent: Color,
    visual: ButtonVisual,
) {
    let style = theme::modal_button_style(accent, visual);
    render_button_frame(f, area, Line::from(label), &style);
}

/// Bordered button with a pre-styled label line centred inside.
fn render_button_frame(f: &mut Frame, area: Rect, label: Line, style: &theme::ButtonStyle) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(style.border)
        .style(style.fill);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let para = Paragraph::new(label)
        .style(style.label)
        .alignment(Alignment::Center);
    f.render_widget(para, inner);
}

// ---------------------------------------------------- status

fn render_status(f: &mut Frame, area: Rect, state: &mut AppState) {
    state.geom.update_notice = None;
    let style = match state.status_kind {
        StatusKind::Info => theme::status_info_style(),
        StatusKind::Error => theme::status_error_style(),
    };
    let notice = state.update_notice.as_deref();
    let msg_w = state.status_message.chars().count() as u16;
    let note_w = notice.map(|n| n.chars().count() as u16).unwrap_or(0);
    let fits_beside = note_w > 0 && msg_w + 2 + note_w <= area.width;
    let is_idle = matches!(state.status_kind, StatusKind::Info) && state.status_message.is_empty();

    // The update note never hides an error or an action result. It
    // sits right-aligned next to the message when there's room, and
    // otherwise takes the row only while the message is the idle hint.
    if let Some(n) = notice.filter(|_| !fits_beside && is_idle) {
        let p = Paragraph::new(n.to_string()).style(theme::status_update_style());
        f.render_widget(p, area);
        state.geom.update_notice = Some(Rect::new(area.x, area.y, note_w.min(area.width), 1));
        return;
    }
    let p = Paragraph::new(state.status_message.clone()).style(style);
    f.render_widget(p, area);
    if let Some(n) =
        notice.filter(|_| fits_beside && !matches!(state.status_kind, StatusKind::Error))
    {
        let rect = Rect::new(area.x + area.width - note_w, area.y, note_w, area.height);
        let p = Paragraph::new(n.to_string()).style(theme::status_update_style());
        f.render_widget(p, rect);
        state.geom.update_notice = Some(rect);
    }
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
        Line::from(Span::styled(
            format!("Really delete session '{}'?", name),
            theme::modal_body_style(),
        )),
        Line::from(Span::styled(
            "This cannot be undone.",
            theme::modal_subtle_style(),
        )),
    ];
    render_modal(
        f,
        area,
        state,
        &title,
        body,
        ("Back", palette::SAPPHIRE),
        ("Delete", palette::RED),
        matches!(focus, DeleteFocus::Cancel),
        matches!(focus, DeleteFocus::Confirm),
    );
}

fn render_rename(f: &mut Frame, area: Rect, state: &mut AppState, original: &str, input: &str) {
    let title = format!(" Rename session '{}' ", original);
    // Field width: room for the name plus some typing headroom.
    let field_w = (input.chars().count() as u16 + 12).clamp(24, 60);
    let mut field: Vec<Span> = Vec::new();
    field.push(Span::styled(" ", theme::modal_input_style()));
    field.push(Span::styled(input.to_string(), theme::modal_input_style()));
    field.push(Span::styled(" ", theme::modal_input_cursor_style()));
    let used = input.chars().count() as u16 + 2;
    if field_w > used {
        field.push(Span::styled(
            " ".repeat((field_w - used) as usize),
            theme::modal_input_style(),
        ));
    }
    let problem = crate::tmux::session_name_problem(input);
    let body = vec![
        Line::from(""),
        Line::from(Span::styled("New name:", theme::modal_body_style())),
        Line::from(""),
        Line::from(field),
        Line::from(""),
        Line::from(Span::styled(
            problem.unwrap_or("Enter renames · Esc cancels · Ctrl-U clears"),
            theme::modal_subtle_style(),
        )),
    ];
    render_modal(
        f,
        area,
        state,
        &title,
        body,
        ("Cancel (Esc)", palette::OVERLAY1),
        ("Rename (Enter)", palette::TEAL),
        false,
        true,
    );
}

fn render_update(
    f: &mut Frame,
    area: Rect,
    state: &mut AppState,
    plan: &crate::update::UpdatePlan,
) {
    let title = " Update tu ".to_string();
    let command = plan.command();
    let mut body = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("tu {}  →  {}", crate::update::CURRENT_VERSION, plan.latest),
            theme::modal_body_style(),
        )),
        Line::from(Span::styled(
            format!("installed via {}", plan.method.describe()),
            theme::modal_subtle_style(),
        )),
        Line::from(""),
    ];
    match &command {
        Some(cmd) => {
            body.push(Line::from(Span::styled(
                "tu will close and run:",
                theme::modal_body_style(),
            )));
            body.push(Line::from(Span::styled(
                format!("  {}", shorten(cmd, 70)),
                theme::modal_directive_style(),
            )));
            body.push(Line::from(""));
            body.push(Line::from(Span::styled(
                "Enter updates · Esc later",
                theme::modal_subtle_style(),
            )));
        }
        None => {
            body.push(Line::from(Span::styled(
                "Download the new version from",
                theme::modal_body_style(),
            )));
            body.push(Line::from(Span::styled(
                format!("  {}", crate::update::LATEST_URL),
                theme::modal_directive_style(),
            )));
        }
    }
    let (secondary, accent) = if command.is_some() {
        ("Update (Enter)", palette::GREEN)
    } else {
        ("OK", palette::OVERLAY1)
    };
    render_modal(
        f,
        area,
        state,
        &title,
        body,
        ("Later (Esc)", palette::OVERLAY1),
        (secondary, accent),
        false,
        true,
    );
}

/// Trim a long shell command for display, keeping the start.
fn shorten(s: &str, max: usize) -> String {
    let flat = s.replace('\n', " ");
    if flat.chars().count() <= max {
        flat
    } else {
        let head: String = flat.chars().take(max - 1).collect();
        format!("{head}…")
    }
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
        Line::from(Span::styled(
            "Add the following to your ~/.tmux.conf?",
            theme::modal_body_style(),
        )),
        Line::from(""),
    ];
    for line in directive_lines {
        body.push(
            Line::from(Span::styled(
                format!("  {line}"),
                theme::modal_directive_style(),
            ))
            .alignment(Alignment::Left),
        );
    }
    body.push(Line::from(""));
    body.push(Line::from(Span::styled(
        "We'll patch the live tmux server too.",
        theme::modal_subtle_style(),
    )));

    render_modal(
        f,
        area,
        state,
        &title,
        body,
        ("Yes, add (y)", palette::GREEN),
        ("Later (n)", palette::OVERLAY1),
        matches!(focus, ConfFocus::Yes),
        matches!(focus, ConfFocus::Later),
    );
}

fn render_restart_notice(f: &mut Frame, area: Rect, state: &mut AppState, message: &str) {
    let title = " All set — restart tu ".to_string();
    let mut body = vec![Line::from("")];
    for paragraph in message.split("\n\n") {
        for line in paragraph.lines() {
            body.push(Line::from(Span::styled(
                line.to_string(),
                theme::modal_body_style(),
            )));
        }
        body.push(Line::from(""));
    }
    body.push(Line::from(Span::styled(
        "Press Enter to quit, then run `tu` again.",
        theme::modal_subtle_style(),
    )));

    let widest_line = body.iter().map(|l| l.width() as u16).max().unwrap_or(40);
    let width = widest_line.max(48).min(area.width.saturating_sub(4));
    // 2 border + 2 margin + 3 button row = 7 rows around the body.
    let height = (body.len() as u16 + 7).min(area.height.saturating_sub(2));
    let modal_area = centered_rect(width + 4, height, area);

    f.render_widget(Clear, modal_area);

    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(theme::modal_border_style())
        .style(theme::modal_bg())
        .title(Span::styled(title, theme::modal_title_style()));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(2), Constraint::Length(3)])
        .split(inner);

    let body_para = Paragraph::new(body)
        .alignment(Alignment::Center)
        .style(theme::modal_bg());
    f.render_widget(body_para, layout[0]);

    let label = "OK (Enter)";
    let bw = (label.chars().count() as u16) + 4;
    let row = layout[1];
    let start_x = row.x + row.width.saturating_sub(bw) / 2;
    let btn_rect = Rect::new(start_x, row.y, bw, row.height);

    // Only one button: store as `modal_primary`, clear `modal_secondary`
    // so a stray click on the (now non-existent) right button does nothing.
    state.geom.modal_primary = Some(btn_rect);
    state.geom.modal_secondary = None;

    let visual = modal_button_visual(state, HitTarget::ModalPrimary, true);
    render_modal_button(f, btn_rect, label, palette::GREEN, visual);
}

#[allow(clippy::too_many_arguments)]
fn render_modal(
    f: &mut Frame,
    area: Rect,
    state: &mut AppState,
    title: &str,
    body: Vec<Line>,
    primary_btn: (&str, Color),
    secondary_btn: (&str, Color),
    primary_focused: bool,
    secondary_focused: bool,
) {
    let widest_line = body.iter().map(|l| l.width() as u16).max().unwrap_or(40);
    let width = widest_line.max(48).min(area.width.saturating_sub(4));
    // 2 border + 2 margin + 3 button row = 7 rows around the body.
    let height = (body.len() as u16 + 7).min(area.height.saturating_sub(2));
    let modal_area = centered_rect(width + 4, height, area);

    f.render_widget(Clear, modal_area);

    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(theme::modal_border_style())
        .style(theme::modal_bg())
        .title(Span::styled(title.to_string(), theme::modal_title_style()));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(2), Constraint::Length(3)])
        .split(inner);

    let body_para = Paragraph::new(body)
        .alignment(Alignment::Center)
        .style(theme::modal_bg());
    f.render_widget(body_para, layout[0]);

    // ---- buttons row inside the modal ----
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

    render_modal_button(
        f,
        primary_rect,
        primary_btn.0,
        primary_btn.1,
        primary_visual,
    );
    render_modal_button(
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

#[cfg(test)]
mod tier_tests {
    use super::*;

    #[test]
    fn tier_follows_available_width() {
        assert_eq!(tier_total(ButtonTier::Full), 74);
        assert_eq!(tier_total(ButtonTier::Word), 56);
        assert_eq!(tier_total(ButtonTier::KeyBoxed), 32);
        assert_eq!(tier_total(ButtonTier::KeyBare), 22);
        assert_eq!(button_tier(120), ButtonTier::Full);
        assert_eq!(button_tier(76), ButtonTier::Full, "80-column pane");
        assert_eq!(button_tier(74), ButtonTier::Full);
        assert_eq!(button_tier(73), ButtonTier::Word);
        assert_eq!(button_tier(56), ButtonTier::Word);
        assert_eq!(button_tier(55), ButtonTier::KeyBoxed);
        assert_eq!(button_tier(32), ButtonTier::KeyBoxed);
        assert_eq!(button_tier(31), ButtonTier::KeyBare);
        assert_eq!(button_tier(10), ButtonTier::KeyBare);
    }
}
