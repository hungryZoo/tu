//! Colour scheme for the ratatui port.
//!
//! Built on the [Catppuccin Mocha](https://catppuccin.com/palette) palette.
//! Every button shares an *identical* state transition recipe — they
//! pick up a per-button accent colour for their borders and pressed
//! fill, but the surface tones used for hover/focus are the same
//! across the row. That gives the interaction a uniform "press the
//! key, the surface lifts a step" feel even though New is green,
//! Detach is peach, and Delete is red.

use ratatui::style::{Color, Modifier, Style};

use crate::state::ButtonId;

pub mod palette {
    //! Catppuccin Mocha — the same hexes the upstream theme ships.
    use ratatui::style::Color;

    // -------- background ramp (darkest → lightest) --------
    pub const CRUST: Color = Color::Rgb(17, 17, 27);
    pub const MANTLE: Color = Color::Rgb(24, 24, 37);
    pub const BASE: Color = Color::Rgb(30, 30, 46);
    pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
    pub const SURFACE1: Color = Color::Rgb(69, 71, 90);
    pub const SURFACE2: Color = Color::Rgb(88, 91, 112);

    // -------- text ramp (mutest → loudest) --------
    pub const OVERLAY0: Color = Color::Rgb(108, 112, 134);
    pub const OVERLAY1: Color = Color::Rgb(127, 132, 156);
    pub const OVERLAY2: Color = Color::Rgb(147, 153, 178);
    pub const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
    pub const SUBTEXT1: Color = Color::Rgb(186, 194, 222);
    pub const TEXT: Color = Color::Rgb(205, 214, 244);

    // -------- accent ramp --------
    pub const LAVENDER: Color = Color::Rgb(180, 190, 254);
    pub const BLUE: Color = Color::Rgb(137, 180, 250);
    pub const SAPPHIRE: Color = Color::Rgb(116, 199, 236);
    pub const SKY: Color = Color::Rgb(137, 220, 235);
    pub const TEAL: Color = Color::Rgb(148, 226, 213);
    pub const GREEN: Color = Color::Rgb(166, 227, 161);
    pub const YELLOW: Color = Color::Rgb(249, 226, 175);
    pub const PEACH: Color = Color::Rgb(250, 179, 135);
    pub const MAROON: Color = Color::Rgb(235, 160, 172);
    pub const RED: Color = Color::Rgb(243, 139, 168);
    pub const MAUVE: Color = Color::Rgb(203, 166, 247);
    pub const PINK: Color = Color::Rgb(245, 194, 231);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVisual {
    Idle,
    Hover,
    Focus,
    Pressed,
    Disabled,
}

impl ButtonId {
    /// Each button claims its own accent colour, picked so the five
    /// buttons read left-to-right as a calm spectrum: green (create)
    /// → blue (connect) → peach (warm caution) → grey (calm exit) →
    /// red (destructive).
    pub fn accent(self) -> Color {
        match self {
            ButtonId::New => palette::GREEN,
            ButtonId::Attach => palette::BLUE,
            ButtonId::Rename => palette::TEAL,
            ButtonId::Detach => palette::PEACH,
            ButtonId::Close => palette::RED,
            ButtonId::Delete => palette::RED,
        }
    }
}

pub struct ButtonStyle {
    /// Border characters' fg.
    pub border: Style,
    /// Background fill applied to both border and interior cells.
    pub fill: Style,
    /// Style applied to the centred label text.
    pub label: Style,
}

/// The single source of truth for "what does a button look like in
/// state X for accent Y?". Same recipe for every button — Hover
/// always lifts to Surface0, Focus always lifts to Surface1 + bold,
/// Pressed always fills with the accent — so an interaction across
/// the row looks visually consistent.
pub fn button_style(accent: Color, visual: ButtonVisual) -> ButtonStyle {
    let (border_fg, fill_bg, label_fg, border_bold, label_bold) = match visual {
        // Idle — only the border carries the accent. Label is muted
        // so the row at rest reads as colourful but quiet.
        ButtonVisual::Idle => (accent, palette::BASE, palette::SUBTEXT1, false, false),

        // Hover — the surface "lifts" one step (Surface0). Border
        // sharpens, label brightens to TEXT.
        ButtonVisual::Hover => (accent, palette::SURFACE0, palette::TEXT, true, false),

        // Focus (keyboard) — lifts two steps (Surface1) and bolds the
        // label so it's distinguishable from a hover.
        ButtonVisual::Focus => (accent, palette::SURFACE1, palette::TEXT, true, true),

        // Pressed — full inverted accent. Reads as "the button is
        // being pushed in right now".
        ButtonVisual::Pressed => (accent, accent, palette::CRUST, true, true),

        // Disabled — drops out of the colour story entirely.
        ButtonVisual::Disabled => (
            palette::OVERLAY0,
            palette::BASE,
            palette::OVERLAY0,
            false,
            false,
        ),
    };

    let mut border = Style::default().fg(border_fg).bg(fill_bg);
    if border_bold {
        border = border.add_modifier(Modifier::BOLD);
    }

    let fill = Style::default().bg(fill_bg);

    let mut label = Style::default().fg(label_fg).bg(fill_bg);
    if label_bold {
        label = label.add_modifier(Modifier::BOLD);
    }

    ButtonStyle {
        border,
        fill,
        label,
    }
}

// ----------------------------------------------------- chrome

pub fn window_bg() -> Style {
    Style::default().bg(palette::BASE)
}

pub fn outer_border_style(inside_tmux: bool) -> Style {
    let accent = if inside_tmux {
        palette::MAUVE
    } else {
        palette::PEACH
    };
    Style::default()
        .fg(accent)
        .bg(palette::BASE)
        .add_modifier(Modifier::BOLD)
}

pub fn title_brand_style() -> Style {
    Style::default()
        .fg(palette::CRUST)
        .bg(palette::MAUVE)
        .add_modifier(Modifier::BOLD)
}

/// Top-right close chip: a filled block like the brand chip, so it
/// reads as a button even without a border.
pub fn close_chip_style(visual: ButtonVisual) -> Style {
    // Black block with white text at rest, so it doesn't read as a
    // selected / focused control (those use the grey surfaces).
    // Hover and focus turn the block red; pressed inverts it.
    let (fg, bg) = match visual {
        ButtonVisual::Idle => (palette::TEXT, palette::CRUST),
        ButtonVisual::Hover => (palette::CRUST, palette::RED),
        ButtonVisual::Focus => (palette::CRUST, palette::RED),
        ButtonVisual::Pressed => (palette::RED, palette::CRUST),
        ButtonVisual::Disabled => (palette::OVERLAY0, palette::CRUST),
    };
    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
}

// ----------------------------------------------------- section labels

pub fn section_marker_style() -> Style {
    Style::default().fg(palette::MAUVE).bg(palette::BASE)
}

pub fn section_label_style() -> Style {
    Style::default()
        .fg(palette::LAVENDER)
        .bg(palette::BASE)
        .add_modifier(Modifier::BOLD)
}

// ----------------------------------------------------- list

pub fn list_block_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(palette::LAVENDER)
            .bg(palette::BASE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette::SURFACE2).bg(palette::BASE)
    }
}

pub fn list_row_default_style() -> Style {
    Style::default().fg(palette::SUBTEXT1).bg(palette::BASE)
}

pub fn list_row_dim_style() -> Style {
    Style::default().fg(palette::OVERLAY1).bg(palette::BASE)
}

pub fn list_selected_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(palette::CRUST)
            .bg(palette::LAVENDER)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(palette::LAVENDER)
            .bg(palette::SURFACE0)
            .add_modifier(Modifier::BOLD)
    }
}

pub fn list_hover_style() -> Style {
    Style::default().fg(palette::TEXT).bg(palette::SURFACE0)
}

pub fn list_attached_marker_style() -> Style {
    Style::default()
        .fg(palette::GREEN)
        .bg(palette::BASE)
        .add_modifier(Modifier::BOLD)
}

pub fn list_empty_style() -> Style {
    Style::default()
        .fg(palette::OVERLAY1)
        .bg(palette::BASE)
        .add_modifier(Modifier::ITALIC)
}

// ----------------------------------------------------- hint + status

/// Key-only label tier: no border, just the key. It takes the colour the
/// border would have had, and the same fill as a boxed button so a
/// hovered / focused key still shows as a grey block (accent when
/// pressed, muted when disabled).
pub fn bare_button_style(accent: Color, visual: ButtonVisual) -> Style {
    let boxed = button_style(accent, visual);
    match visual {
        ButtonVisual::Pressed => boxed.label,
        ButtonVisual::Disabled => boxed.label,
        _ => boxed.border.add_modifier(Modifier::BOLD),
    }
}

/// The shortcut key inside a button label: the hint-bar yellow, bold,
/// on the button's own fill so it reads as part of the label.
pub fn button_key_style(label: Style) -> Style {
    label.fg(palette::YELLOW).add_modifier(Modifier::BOLD)
}

pub fn status_info_style() -> Style {
    Style::default()
        .fg(palette::SUBTEXT0)
        .bg(palette::BASE)
        .add_modifier(Modifier::ITALIC)
}

pub fn status_update_style() -> Style {
    // Drawn over the info message, whose italic would otherwise bleed
    // through (ratatui patches modifiers rather than replacing them).
    Style::default()
        .fg(palette::PEACH)
        .bg(palette::BASE)
        .add_modifier(Modifier::BOLD)
        .remove_modifier(Modifier::ITALIC)
}

pub fn status_error_style() -> Style {
    Style::default()
        .fg(palette::RED)
        .bg(palette::BASE)
        .add_modifier(Modifier::BOLD)
}

// ----------------------------------------------------- modal

pub fn modal_bg() -> Style {
    Style::default().bg(palette::CRUST)
}

pub fn modal_border_style() -> Style {
    Style::default()
        .fg(palette::YELLOW)
        .bg(palette::CRUST)
        .add_modifier(Modifier::BOLD)
}

pub fn modal_title_style() -> Style {
    Style::default()
        .fg(palette::CRUST)
        .bg(palette::YELLOW)
        .add_modifier(Modifier::BOLD)
}

pub fn modal_body_style() -> Style {
    Style::default().fg(palette::TEXT).bg(palette::CRUST)
}

pub fn modal_subtle_style() -> Style {
    Style::default()
        .fg(palette::OVERLAY1)
        .bg(palette::CRUST)
        .add_modifier(Modifier::ITALIC)
}

/// The text field inside the Rename modal: a filled strip so its
/// extent is obvious, with the cursor drawn as a reverse-video cell.
pub fn modal_input_style() -> Style {
    Style::default().fg(palette::TEXT).bg(palette::SURFACE0)
}

pub fn modal_input_cursor_style() -> Style {
    Style::default().fg(palette::CRUST).bg(palette::TEAL)
}

pub fn modal_directive_style() -> Style {
    Style::default()
        .fg(palette::PEACH)
        .bg(palette::CRUST)
        .add_modifier(Modifier::BOLD)
}

/// Buttons rendered inside a modal sit on top of `palette::CRUST`
/// rather than `palette::BASE`, so the idle/disabled fills shift
/// accordingly. Otherwise identical to [`button_style`].
pub fn modal_button_style(accent: Color, visual: ButtonVisual) -> ButtonStyle {
    let (border_fg, fill_bg, label_fg, border_bold, label_bold) = match visual {
        ButtonVisual::Idle => (accent, palette::CRUST, palette::SUBTEXT1, false, false),
        ButtonVisual::Hover => (accent, palette::SURFACE0, palette::TEXT, true, false),
        ButtonVisual::Focus => (accent, palette::SURFACE1, palette::TEXT, true, true),
        ButtonVisual::Pressed => (accent, accent, palette::CRUST, true, true),
        ButtonVisual::Disabled => (
            palette::OVERLAY0,
            palette::CRUST,
            palette::OVERLAY0,
            false,
            false,
        ),
    };
    let mut border = Style::default().fg(border_fg).bg(fill_bg);
    if border_bold {
        border = border.add_modifier(Modifier::BOLD);
    }
    let fill = Style::default().bg(fill_bg);
    let mut label = Style::default().fg(label_fg).bg(fill_bg);
    if label_bold {
        label = label.add_modifier(Modifier::BOLD);
    }
    ButtonStyle {
        border,
        fill,
        label,
    }
}
