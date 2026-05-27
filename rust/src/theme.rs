//! Colour scheme for the ratatui port.
//!
//! Buttons carry a *variant* (Default / Grey / Danger) and pick up
//! their style from `button_colors`. The same matrix encodes the four
//! visual states a button can be in: idle, hover (mouse over), focus
//! (keyboard), and pressed (mouse-down latched on the same widget).

use ratatui::style::{Color, Modifier, Style};

use crate::state::ButtonId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Default,
    Grey,
    Danger,
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
    pub fn variant(self) -> ButtonVariant {
        match self {
            // Quit is intentionally muted so it doesn't look like the
            // primary action; Delete is loud so it's hard to misclick.
            ButtonId::Quit => ButtonVariant::Grey,
            ButtonId::Delete => ButtonVariant::Danger,
            _ => ButtonVariant::Default,
        }
    }
}

pub struct ButtonStyle {
    pub border: Style,
    pub label: Style,
}

pub fn button_style(variant: ButtonVariant, visual: ButtonVisual) -> ButtonStyle {
    let (border_fg, label_fg, label_bg, bold) = match (variant, visual) {
        // Idle baseline — variant only colours the border at rest.
        (ButtonVariant::Default, ButtonVisual::Idle) => {
            (Color::Cyan, Color::White, Color::Reset, false)
        }
        (ButtonVariant::Grey, ButtonVisual::Idle) => {
            (Color::DarkGray, Color::Gray, Color::Reset, false)
        }
        (ButtonVariant::Danger, ButtonVisual::Idle) => {
            (Color::Red, Color::LightRed, Color::Reset, false)
        }

        // Hover — brighten the border, keep the label readable.
        (ButtonVariant::Default, ButtonVisual::Hover) => {
            (Color::LightCyan, Color::White, Color::Rgb(0, 40, 60), true)
        }
        (ButtonVariant::Grey, ButtonVisual::Hover) => {
            (Color::White, Color::White, Color::Rgb(50, 50, 50), true)
        }
        (ButtonVariant::Danger, ButtonVisual::Hover) => {
            (Color::LightRed, Color::White, Color::Rgb(60, 0, 0), true)
        }

        // Focus — keyboard cursor lives here, render inverted.
        (ButtonVariant::Default, ButtonVisual::Focus) => {
            (Color::LightCyan, Color::Black, Color::LightCyan, true)
        }
        (ButtonVariant::Grey, ButtonVisual::Focus) => {
            (Color::White, Color::Black, Color::Gray, true)
        }
        (ButtonVariant::Danger, ButtonVisual::Focus) => {
            (Color::LightRed, Color::White, Color::Red, true)
        }

        // Pressed — same inverted look but a half-step deeper.
        (ButtonVariant::Default, ButtonVisual::Pressed) => {
            (Color::Cyan, Color::Black, Color::Cyan, true)
        }
        (ButtonVariant::Grey, ButtonVisual::Pressed) => {
            (Color::Gray, Color::Black, Color::DarkGray, true)
        }
        (ButtonVariant::Danger, ButtonVisual::Pressed) => {
            (Color::Red, Color::White, Color::Rgb(100, 0, 0), true)
        }

        // Disabled — flat, low contrast, no decoration.
        (_, ButtonVisual::Disabled) => (Color::DarkGray, Color::DarkGray, Color::Reset, false),
    };

    let mut label = Style::default().fg(label_fg).bg(label_bg);
    if bold {
        label = label.add_modifier(Modifier::BOLD);
    }
    ButtonStyle {
        border: Style::default().fg(border_fg),
        label,
    }
}

pub fn outer_border_style(inside_tmux: bool) -> Style {
    Style::default().fg(if inside_tmux {
        Color::Cyan
    } else {
        Color::Magenta
    })
}

pub fn list_selected_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }
}

pub fn list_hover_style() -> Style {
    Style::default().fg(Color::White).bg(Color::Rgb(40, 40, 40))
}

pub fn list_attached_marker_style() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}

pub fn hint_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn status_info_style() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn status_error_style() -> Style {
    Style::default()
        .fg(Color::LightRed)
        .add_modifier(Modifier::BOLD)
}

pub fn modal_border_style() -> Style {
    Style::default().fg(Color::Yellow)
}

pub fn modal_dim_overlay() -> Style {
    Style::default().bg(Color::Black).fg(Color::DarkGray)
}
