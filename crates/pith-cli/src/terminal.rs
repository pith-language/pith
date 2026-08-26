//! Terminal capability and theme detection at the process boundary.

use std::io::{self, IsTerminal as _, Stdout, Write};
use std::time::Duration;

use pith_output::palette::{Palette, Role, TerminalTheme};
use terminal_colorsaurus::{ColorPalette, QueryOptions, ThemeMode, color_palette};
use termprofile::{DetectorSettings, TermProfile};

/// Maximum theme-query latency before the balanced palette takes over.
///
/// Supporting terminals usually answer immediately. A short bound keeps an
/// unreachable outer terminal from delaying every pith command over a remote
/// or multiplexed connection.
const THEME_QUERY_TIMEOUT: Duration = Duration::from_millis(100);

/// Everything output rendering needs to know about stdout.
#[derive(Clone, Debug)]
pub struct OutputTerminal {
    profile: TermProfile,
    palette: Palette,
    stdout_is_terminal: bool,
    pretty_by_default: bool,
    theme_probe: ThemeProbe,
}

impl OutputTerminal {
    /// Detect stdout's color depth and, when safe, its current theme.
    pub fn detect(stdout: &Stdout) -> Self {
        // termprofile detection is passive: environment, terminfo, and tmux
        // metadata. The separate OSC theme query below is tightly gated.
        let profile = TermProfile::detect(stdout, DetectorSettings::default());
        let stdout_is_terminal = stdout.is_terminal();
        let pretty_by_default = stdout_is_terminal && profile != TermProfile::NoTty;
        let theme_probe = if pretty_by_default && profile_supports_color(profile) {
            probe_theme()
        } else {
            ThemeProbe::NotAttempted
        };
        let palette = theme_probe.theme().map_or_else(
            || Palette::for_profile(profile),
            |theme| Palette::for_terminal(profile, theme),
        );

        Self {
            profile,
            palette,
            stdout_is_terminal,
            pretty_by_default,
            theme_probe,
        }
    }

    /// Detected output color depth.
    pub const fn profile(&self) -> TermProfile {
        self.profile
    }

    /// Semantic palette adapted to the detected terminal.
    pub const fn palette(&self) -> Palette {
        self.palette
    }

    /// Whether human-readable output should use the pretty shape by default.
    pub const fn pretty_by_default(&self) -> bool {
        self.pretty_by_default
    }

    /// Write the deliberately unstable terminal-detection report.
    ///
    /// Unlike normal output, this does not use `OutputRecord` or the query API.
    /// An explicit debug invocation attempts a theme query even when normal
    /// output would skip it, which lets `pith debug terminal` explain a pipe or
    /// `NO_COLOR` environment without changing their normal behavior.
    pub fn write_debug_report(&self, mut out: impl Write) -> io::Result<()> {
        let probe = match &self.theme_probe {
            ThemeProbe::NotAttempted => probe_theme(),
            attempted => attempted.clone(),
        };

        writeln!(out, "warning=unstable-debug-output")?;
        writeln!(out, "stdout_is_terminal={}", self.stdout_is_terminal)?;
        writeln!(out, "color_profile={}", profile_name(self.profile))?;
        match probe {
            ThemeProbe::Detected(theme) => {
                writeln!(out, "theme_query=detected")?;
                writeln!(out, "theme={}", theme.mode_name())?;
                write_color(
                    &mut out,
                    "foreground",
                    theme.foreground_16,
                    theme.foreground_8,
                )?;
                write_color(
                    &mut out,
                    "background",
                    theme.background_16,
                    theme.background_8,
                )?;
            }
            ThemeProbe::Unavailable(error) => {
                writeln!(out, "theme_query=unavailable")?;
                writeln!(out, "theme=unknown")?;
                writeln!(out, "theme_error={}", one_line(&error))?;
            }
            ThemeProbe::NotAttempted => {
                // `probe_theme` itself always records success or failure.
                writeln!(out, "theme_query=unavailable")?;
                writeln!(out, "theme=unknown")?;
            }
        }

        for (name, role) in [
            ("heading", Role::Heading),
            ("success", Role::Success),
            ("failure", Role::Failure),
            ("attention", Role::Attention),
            ("reuse", Role::Reuse),
            ("muted", Role::Muted),
            ("literal", Role::Literal),
            ("placeholder", Role::Placeholder),
        ] {
            writeln!(
                out,
                "palette.{name}={:?}",
                self.palette.style(role).get_fg_color()
            )?;
        }
        out.flush()
    }
}

const fn profile_supports_color(profile: TermProfile) -> bool {
    matches!(
        profile,
        TermProfile::Ansi16 | TermProfile::Ansi256 | TermProfile::TrueColor
    )
}

#[derive(Clone, Debug)]
enum ThemeProbe {
    NotAttempted,
    Detected(DetectedTheme),
    Unavailable(Box<str>),
}

impl ThemeProbe {
    fn theme(&self) -> Option<TerminalTheme> {
        match self {
            Self::Detected(theme) => Some(theme.theme),
            Self::NotAttempted | Self::Unavailable(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
struct DetectedTheme {
    theme: TerminalTheme,
    mode: ThemeMode,
    foreground_16: (u16, u16, u16),
    foreground_8: (u8, u8, u8),
    background_16: (u16, u16, u16),
    background_8: (u8, u8, u8),
}

impl DetectedTheme {
    fn from_palette(colors: ColorPalette) -> Self {
        let foreground_16 = (
            colors.foreground.r,
            colors.foreground.g,
            colors.foreground.b,
        );
        let foreground_8 = colors.foreground.scale_to_8bit();
        let background_16 = (
            colors.background.r,
            colors.background.g,
            colors.background.b,
        );
        let background_8 = colors.background.scale_to_8bit();
        Self {
            theme: TerminalTheme::new(foreground_8, background_8),
            mode: colors.theme_mode(),
            foreground_16,
            foreground_8,
            background_16,
            background_8,
        }
    }

    const fn mode_name(&self) -> &'static str {
        match self.mode {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
}

fn probe_theme() -> ThemeProbe {
    let mut options = QueryOptions::default();
    options.timeout = THEME_QUERY_TIMEOUT;
    match color_palette(options) {
        Ok(colors) => ThemeProbe::Detected(DetectedTheme::from_palette(colors)),
        Err(error) => {
            tracing::debug!(%error, "terminal theme detection unavailable");
            ThemeProbe::Unavailable(error.to_string().into_boxed_str())
        }
    }
}

const fn profile_name(profile: TermProfile) -> &'static str {
    match profile {
        TermProfile::NoTty => "no_tty",
        TermProfile::NoColor => "no_color",
        TermProfile::Ansi16 => "ansi16",
        TermProfile::Ansi256 => "ansi256",
        TermProfile::TrueColor => "truecolor",
    }
}

fn write_color(
    out: &mut impl Write,
    name: &str,
    rgb_16: (u16, u16, u16),
    rgb_8: (u8, u8, u8),
) -> io::Result<()> {
    writeln!(out, "{name}_rgb16={},{},{}", rgb_16.0, rgb_16.1, rgb_16.2)?;
    writeln!(
        out,
        "{name}_rgb8=#{:02x}{:02x}{:02x}",
        rgb_8.0, rgb_8.1, rgb_8.2
    )
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_queries_are_only_useful_for_color_profiles() {
        assert!(!profile_supports_color(TermProfile::NoTty));
        assert!(!profile_supports_color(TermProfile::NoColor));
        assert!(profile_supports_color(TermProfile::Ansi16));
        assert!(profile_supports_color(TermProfile::Ansi256));
        assert!(profile_supports_color(TermProfile::TrueColor));
    }

    #[test]
    fn debug_report_names_the_raw_theme_and_selected_palette()
    -> Result<(), Box<dyn std::error::Error>> {
        let theme = DetectedTheme {
            theme: TerminalTheme::new((238, 238, 238), (17, 34, 51)),
            mode: ThemeMode::Dark,
            foreground_16: (61_166, 61_166, 61_166),
            foreground_8: (238, 238, 238),
            background_16: (4_369, 8_738, 13_107),
            background_8: (17, 34, 51),
        };
        let terminal = OutputTerminal {
            profile: TermProfile::TrueColor,
            palette: Palette::for_terminal(TermProfile::TrueColor, theme.theme),
            stdout_is_terminal: true,
            pretty_by_default: true,
            theme_probe: ThemeProbe::Detected(theme),
        };
        let mut output = Vec::new();

        terminal.write_debug_report(&mut output)?;

        let output = String::from_utf8(output)?;
        assert!(
            output.contains("warning=unstable-debug-output\n"),
            "{output}"
        );
        assert!(output.contains("theme=dark\n"), "{output}");
        assert!(output.contains("foreground_rgb8=#eeeeee\n"), "{output}");
        assert!(output.contains("background_rgb8=#112233\n"), "{output}");
        assert!(output.contains("palette.success=Some(Rgb("), "{output}");
        Ok(())
    }
}
