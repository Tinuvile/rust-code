//! Color theme definitions matching the original TypeScript theme system.
//!
//! Ref: src/utils/theme.ts

use ratatui::style::Color;

/// Spinner animation frames (matches original Ink spinner).
pub const SPINNER_FRAMES: &[&str] = &["·", "✢", "✳", "✶", "✻", "✽", "✻", "✶", "✳", "✢"];

/// A resolved color theme. All colors are concrete terminal `Color` values.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Brand orange — used for spinner, prompt border, "Claude:" prefix.
    pub claude: Color,
    /// Permission blue — used for permission dialogs.
    pub permission: Color,
    /// Success green.
    pub success: Color,
    /// Error red.
    pub error: Color,
    /// Warning amber.
    pub warning: Color,
    /// Primary foreground text.
    pub text: Color,
    /// Dimmed/secondary text (timestamps, labels).
    pub subtle: Color,
    /// Diff added lines.
    pub diff_added: Color,
    /// Diff removed lines.
    pub diff_removed: Color,
    /// Input box border color (matches claude orange in dark theme).
    pub prompt_border: Color,
}

/// Dark theme (default) — matches TypeScript `dark` theme.
pub fn dark_theme() -> Theme {
    Theme {
        claude: Color::Rgb(215, 119, 87),
        permission: Color::Rgb(87, 105, 247),
        success: Color::Rgb(44, 122, 57),
        error: Color::Rgb(171, 43, 63),
        warning: Color::Rgb(200, 150, 40),
        text: Color::White,
        subtle: Color::DarkGray,
        diff_added: Color::Green,
        diff_removed: Color::Red,
        prompt_border: Color::Rgb(215, 119, 87),
    }
}

/// Light theme — inverted foreground/background for bright terminals.
pub fn light_theme() -> Theme {
    Theme {
        claude: Color::Rgb(180, 80, 50),
        permission: Color::Rgb(50, 80, 220),
        success: Color::Rgb(30, 100, 40),
        error: Color::Rgb(140, 30, 50),
        warning: Color::Rgb(160, 110, 20),
        text: Color::Black,
        subtle: Color::Gray,
        diff_added: Color::DarkGray,
        diff_removed: Color::Red,
        prompt_border: Color::Rgb(180, 80, 50),
    }
}

/// Dark ANSI theme — 16-color safe fallback.
pub fn dark_ansi_theme() -> Theme {
    Theme {
        claude: Color::Yellow,
        permission: Color::Blue,
        success: Color::Green,
        error: Color::Red,
        warning: Color::Yellow,
        text: Color::White,
        subtle: Color::DarkGray,
        diff_added: Color::Green,
        diff_removed: Color::Red,
        prompt_border: Color::Yellow,
    }
}
