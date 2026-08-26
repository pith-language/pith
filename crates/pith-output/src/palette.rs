//! One semantic palette, adapted once to the output terminal.
//!
//! The CLI detects a [`TermProfile`] for stdout and gives the resulting
//! [`Palette`] to every human-facing renderer. The renderer asks for a
//! semantic [`Role`], not for a color, so terminal capability never leaks into
//! descriptions and RGB/256/16-color fallbacks cannot drift between call
//! sites. When the terminal reports its current foreground and background,
//! [`TerminalTheme`] also lets richer profiles enforce text contrast against
//! the actual background rather than guessing whether it is light or dark.

use anstyle::{Ansi256Color, AnsiColor, Color, RgbColor, Style};
use termprofile::TermProfile;

/// Minimum contrast used for colored terminal text.
///
/// This is the WCAG AA threshold for ordinary text.
pub const MIN_CONTRAST_RATIO: f32 = 4.5;

/// The terminal's measured default foreground and background colors.
///
/// Detection and terminal I/O stay in the driver. This value is deliberately
/// small so renderers can be tested without owning a terminal.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TerminalTheme {
    foreground: RgbColor,
    background: RgbColor,
}

impl TerminalTheme {
    /// Construct a theme from eight-bit sRGB channel values.
    #[must_use]
    pub const fn new(foreground: (u8, u8, u8), background: (u8, u8, u8)) -> Self {
        Self {
            foreground: RgbColor(foreground.0, foreground.1, foreground.2),
            background: RgbColor(background.0, background.1, background.2),
        }
    }

    /// The terminal's default foreground.
    #[must_use]
    pub const fn foreground(self) -> RgbColor {
        self.foreground
    }

    /// The terminal's default background.
    #[must_use]
    pub const fn background(self) -> RgbColor {
        self.background
    }

    fn has_dark_background(self) -> bool {
        relative_luminance(self.background) < relative_luminance(self.foreground)
    }
}

/// Why a piece of output is styled.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// A section heading, and the name of a phase that finished.
    Heading,
    /// Work that succeeded.
    Success,
    /// Work that failed, and the count of errors in a summary.
    Failure,
    /// A result that needs attention without being a failure.
    Attention,
    /// A result served from earlier work.
    Reuse,
    /// Context the reader can skip.
    Muted,
    /// Something the reader would type verbatim.
    Literal,
    /// Something the reader substitutes in a usage line.
    Placeholder,
}

/// Section-heading role.
pub const HEADING: Role = Role::Heading;
/// Successful-work role.
pub const SUCCESS: Role = Role::Success;
/// Failure role.
pub const FAILURE: Role = Role::Failure;
/// Attention role.
pub const ATTENTION: Role = Role::Attention;
/// Reused-work role.
pub const REUSE: Role = Role::Reuse;
/// Secondary-context role.
pub const MUTED: Role = Role::Muted;
/// Literal-input role.
pub const LITERAL: Role = Role::Literal;
/// Substituted-value role.
pub const PLACEHOLDER: Role = Role::Placeholder;

/// Styles for one detected terminal profile.
///
/// Without theme information, RGB values are terminal-safe tonal adaptations
/// of pith's brand hues. With a measured theme, RGB values are adjusted and
/// ANSI-256 values are selected from the fixed xterm cube to meet
/// [`MIN_CONTRAST_RATIO`] against the actual background. ANSI-16 colors remain
/// symbolic because those slots belong to the user's terminal theme.
#[derive(Copy, Clone, Debug)]
pub struct Palette {
    profile: TermProfile,
    heading: Style,
    success: Style,
    failure: Style,
    attention: Style,
    reuse: Style,
    muted: Style,
    literal: Style,
    placeholder: Style,
}

impl Palette {
    /// Derive all styles once for `profile`.
    #[must_use]
    pub fn for_profile(profile: TermProfile) -> Self {
        Self::derive(profile, None)
    }

    /// Derive all styles for `profile` and a measured terminal `theme`.
    #[must_use]
    pub fn for_terminal(profile: TermProfile, theme: TerminalTheme) -> Self {
        Self::derive(profile, Some(theme))
    }

    fn derive(profile: TermProfile, theme: Option<TerminalTheme>) -> Self {
        let muted = semantic(profile, theme, (116, 120, 108), 243, AnsiColor::BrightBlack);
        // SGR dim has terminal-defined intensity, so do not apply it after a
        // measured contrast color has been selected. The unknown/no-color
        // fallbacks retain the established secondary-text treatment.
        let muted = if theme.is_some() && profile_supports_color(profile) {
            muted
        } else {
            muted.dimmed()
        };

        Self {
            profile,
            heading: Style::new().bold(),
            // Pith green, balanced for both light and dark terminal grounds.
            success: semantic(profile, theme, (79, 128, 95), 29, AnsiColor::Green),
            // A warm red that sits beside pith green without looking neon.
            failure: semantic(profile, theme, (187, 85, 85), 131, AnsiColor::Red).bold(),
            // Warm gold: warning, not generic terminal yellow in richer modes.
            attention: semantic(profile, theme, (147, 111, 25), 130, AnsiColor::Yellow),
            // A quiet blue-green for reused work and informational output.
            reuse: semantic(profile, theme, (64, 128, 135), 30, AnsiColor::Cyan),
            muted,
            // Chartreuse, pith's single accent, deepened for light terminals.
            literal: semantic(profile, theme, (113, 123, 33), 64, AnsiColor::Green).bold(),
            placeholder: semantic(profile, theme, (64, 128, 135), 30, AnsiColor::Cyan),
        }
        .adapt_effects(profile)
    }

    /// The style for one semantic role.
    #[must_use]
    pub const fn style(self, role: Role) -> Style {
        match role {
            Role::Heading => self.heading,
            Role::Success => self.success,
            Role::Failure => self.failure,
            Role::Attention => self.attention,
            Role::Reuse => self.reuse,
            Role::Muted => self.muted,
            Role::Literal => self.literal,
            Role::Placeholder => self.placeholder,
        }
    }

    /// Add emphasis without reintroducing escapes for a non-TTY profile.
    #[must_use]
    pub fn emphasized(self, role: Role) -> Style {
        effects(self.profile, self.style(role).bold())
    }

    fn adapt_effects(mut self, profile: TermProfile) -> Self {
        self.heading = effects(profile, self.heading);
        self.success = effects(profile, self.success);
        self.failure = effects(profile, self.failure);
        self.attention = effects(profile, self.attention);
        self.reuse = effects(profile, self.reuse);
        self.muted = effects(profile, self.muted);
        self.literal = effects(profile, self.literal);
        self.placeholder = effects(profile, self.placeholder);
        self
    }
}

fn semantic(
    profile: TermProfile,
    theme: Option<TerminalTheme>,
    (red, green, blue): (u8, u8, u8),
    ansi_256: u8,
    ansi_16: AnsiColor,
) -> Style {
    let desired = RgbColor(red, green, blue);
    let color = match profile {
        TermProfile::TrueColor => {
            Some(Color::Rgb(theme.map_or(desired, |theme| {
                contrasting_rgb(desired, theme.background)
            })))
        }
        TermProfile::Ansi256 => Some(Color::Ansi256(Ansi256Color(
            theme.map_or(ansi_256, |theme| {
                contrasting_ansi_256(desired, theme.background, ansi_256)
            }),
        ))),
        TermProfile::Ansi16 => Some(Color::Ansi(contrasting_ansi_16(ansi_16, theme))),
        TermProfile::NoColor | TermProfile::NoTty => None,
    };
    Style::new().fg_color(color)
}

const fn profile_supports_color(profile: TermProfile) -> bool {
    matches!(
        profile,
        TermProfile::Ansi16 | TermProfile::Ansi256 | TermProfile::TrueColor
    )
}

fn contrasting_ansi_16(color: AnsiColor, theme: Option<TerminalTheme>) -> AnsiColor {
    match theme.map(TerminalTheme::has_dark_background) {
        Some(true) => bright_variant(color),
        Some(false) if color == AnsiColor::BrightBlack => AnsiColor::Black,
        Some(false) | None => color,
    }
}

const fn bright_variant(color: AnsiColor) -> AnsiColor {
    match color {
        AnsiColor::Black | AnsiColor::BrightBlack => AnsiColor::BrightBlack,
        AnsiColor::Red | AnsiColor::BrightRed => AnsiColor::BrightRed,
        AnsiColor::Green | AnsiColor::BrightGreen => AnsiColor::BrightGreen,
        AnsiColor::Yellow | AnsiColor::BrightYellow => AnsiColor::BrightYellow,
        AnsiColor::Blue | AnsiColor::BrightBlue => AnsiColor::BrightBlue,
        AnsiColor::Magenta | AnsiColor::BrightMagenta => AnsiColor::BrightMagenta,
        AnsiColor::Cyan | AnsiColor::BrightCyan => AnsiColor::BrightCyan,
        AnsiColor::White | AnsiColor::BrightWhite => AnsiColor::BrightWhite,
    }
}

fn contrasting_rgb(desired: RgbColor, background: RgbColor) -> RgbColor {
    if contrast_ratio(desired, background) >= MIN_CONTRAST_RATIO {
        return desired;
    }

    let toward_black = first_contrasting_mix(desired, background, RgbColor(0, 0, 0));
    let toward_white = first_contrasting_mix(desired, background, RgbColor(255, 255, 255));
    match (toward_black, toward_white) {
        (Some(black), Some(white))
            if rgb_distance(desired, black) <= rgb_distance(desired, white) =>
        {
            black
        }
        (Some(_), Some(white)) => white,
        (Some(black), None) => black,
        (None, Some(white)) => white,
        // For any opaque sRGB background, black or white reaches 4.5:1. Keep
        // this total in case rounding behavior ever changes around the bound.
        (None, None) => higher_contrast(RgbColor(0, 0, 0), RgbColor(255, 255, 255), background),
    }
}

fn first_contrasting_mix(
    desired: RgbColor,
    background: RgbColor,
    endpoint: RgbColor,
) -> Option<RgbColor> {
    (1_u16..=255).find_map(|step| {
        let amount = f32::from(step) / 255.0;
        let candidate = mix(desired, endpoint, amount);
        (contrast_ratio(candidate, background) >= MIN_CONTRAST_RATIO).then_some(candidate)
    })
}

fn mix(from: RgbColor, to: RgbColor, amount: f32) -> RgbColor {
    RgbColor(
        mix_channel(from.0, to.0, amount),
        mix_channel(from.1, to.1, amount),
        mix_channel(from.2, to.2, amount),
    )
}

fn mix_channel(from: u8, to: u8, amount: f32) -> u8 {
    (f32::from(from) + (f32::from(to) - f32::from(from)) * amount)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn higher_contrast(first: RgbColor, second: RgbColor, background: RgbColor) -> RgbColor {
    if contrast_ratio(first, background) >= contrast_ratio(second, background) {
        first
    } else {
        second
    }
}

fn contrasting_ansi_256(desired: RgbColor, background: RgbColor, fallback: u8) -> u8 {
    (16_u8..=u8::MAX)
        .filter_map(|index| {
            let candidate = ansi_256_rgb(index);
            (contrast_ratio(candidate, background) >= MIN_CONTRAST_RATIO)
                .then_some((index, rgb_distance(desired, candidate)))
        })
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map_or(fallback, |(index, _)| index)
}

fn ansi_256_rgb(index: u8) -> RgbColor {
    match index {
        16..=231 => {
            let offset = index.saturating_sub(16);
            let red = usize::from(offset / 36);
            let green = usize::from((offset % 36) / 6);
            let blue = usize::from(offset % 6);
            RgbColor(cube_level(red), cube_level(green), cube_level(blue))
        }
        232..=255 => {
            let value = 8_u8.saturating_add(index.saturating_sub(232).saturating_mul(10));
            RgbColor(value, value, value)
        }
        0..=15 => RgbColor(0, 0, 0),
    }
}

fn cube_level(component: usize) -> u8 {
    [0, 95, 135, 175, 215, 255]
        .get(component)
        .copied()
        .unwrap_or_default()
}

fn contrast_ratio(foreground: RgbColor, background: RgbColor) -> f32 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    let lighter = foreground.max(background);
    let darker = foreground.min(background);
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: RgbColor) -> f32 {
    0.2126 * linear_channel(color.0)
        + 0.7152 * linear_channel(color.1)
        + 0.0722 * linear_channel(color.2)
}

fn linear_channel(channel: u8) -> f32 {
    let channel = f32::from(channel) / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn rgb_distance(first: RgbColor, second: RgbColor) -> f32 {
    let red = f32::from(first.0) - f32::from(second.0);
    let green = f32::from(first.1) - f32::from(second.1);
    let blue = f32::from(first.2) - f32::from(second.2);
    red.mul_add(red, green.mul_add(green, blue * blue))
}

fn effects(profile: TermProfile, style: Style) -> Style {
    match profile {
        TermProfile::NoTty => Style::new(),
        TermProfile::NoColor
        | TermProfile::Ansi16
        | TermProfile::Ansi256
        | TermProfile::TrueColor => style,
    }
}

/// Wrap `text` in `style` and its reset.
#[must_use]
pub fn paint(style: Style, text: &str) -> String {
    format!("{style}{text}{style:#}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_color_depth_gets_its_deliberate_success_color() {
        assert_eq!(
            Palette::for_profile(TermProfile::TrueColor)
                .style(Role::Success)
                .get_fg_color(),
            Some(Color::Rgb(RgbColor(79, 128, 95)))
        );
        assert_eq!(
            Palette::for_profile(TermProfile::Ansi256)
                .style(Role::Success)
                .get_fg_color(),
            Some(Color::Ansi256(Ansi256Color(29)))
        );
        assert_eq!(
            Palette::for_profile(TermProfile::Ansi16)
                .style(Role::Success)
                .get_fg_color(),
            Some(Color::Ansi(AnsiColor::Green))
        );
    }

    #[test]
    fn no_color_keeps_emphasis_but_no_tty_emits_no_escapes() {
        let no_color = Palette::for_profile(TermProfile::NoColor);
        assert_eq!(no_color.style(Role::Failure), Style::new().bold());
        assert_eq!(no_color.style(Role::Literal), Style::new().bold());

        let no_tty = Palette::for_profile(TermProfile::NoTty);
        for role in [
            Role::Heading,
            Role::Success,
            Role::Failure,
            Role::Attention,
            Role::Reuse,
            Role::Muted,
            Role::Literal,
            Role::Placeholder,
        ] {
            assert_eq!(no_tty.style(role), Style::new());
        }
    }

    #[test]
    fn measured_rich_colors_meet_the_contrast_floor() -> Result<(), String> {
        let backgrounds = [
            RgbColor(0, 0, 0),
            RgbColor(255, 255, 255),
            RgbColor(0, 43, 54),
            RgbColor(253, 246, 227),
            RgbColor(117, 117, 117),
        ];
        let roles = [
            Role::Success,
            Role::Failure,
            Role::Attention,
            Role::Reuse,
            Role::Muted,
            Role::Literal,
            Role::Placeholder,
        ];

        for background in backgrounds {
            let foreground =
                higher_contrast(RgbColor(0, 0, 0), RgbColor(255, 255, 255), background);
            let theme = TerminalTheme::new(
                (foreground.0, foreground.1, foreground.2),
                (background.0, background.1, background.2),
            );
            for profile in [TermProfile::TrueColor, TermProfile::Ansi256] {
                let palette = Palette::for_terminal(profile, theme);
                for role in roles {
                    let color = match palette.style(role).get_fg_color() {
                        Some(Color::Rgb(color)) => color,
                        Some(Color::Ansi256(Ansi256Color(index))) => ansi_256_rgb(index),
                        other => return Err(format!("{profile:?}/{role:?} produced {other:?}")),
                    };
                    assert!(
                        contrast_ratio(color, background) >= MIN_CONTRAST_RATIO,
                        "{profile:?}/{role:?}: {color:?} against {background:?} has ratio {}",
                        contrast_ratio(color, background)
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn ansi_16_uses_the_terminals_light_or_dark_variant() {
        let dark = TerminalTheme::new((230, 230, 230), (20, 20, 20));
        let light = TerminalTheme::new((20, 20, 20), (245, 245, 245));

        assert_eq!(
            Palette::for_terminal(TermProfile::Ansi16, dark)
                .style(Role::Success)
                .get_fg_color(),
            Some(Color::Ansi(AnsiColor::BrightGreen))
        );
        assert_eq!(
            Palette::for_terminal(TermProfile::Ansi16, light)
                .style(Role::Success)
                .get_fg_color(),
            Some(Color::Ansi(AnsiColor::Green))
        );
        assert_eq!(
            Palette::for_terminal(TermProfile::Ansi16, light)
                .style(Role::Muted)
                .get_fg_color(),
            Some(Color::Ansi(AnsiColor::Black))
        );
    }
}
