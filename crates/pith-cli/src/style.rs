use clap::builder::Styles;
use pith_output::palette::{self, Palette};

pub fn help(palette: Palette) -> Styles {
    Styles::styled()
        .header(palette.style(palette::HEADING))
        .usage(palette.style(palette::HEADING))
        .literal(palette.style(palette::LITERAL))
        .placeholder(palette.style(palette::PLACEHOLDER))
        .error(palette.style(palette::FAILURE))
        .valid(palette.style(palette::SUCCESS))
        .invalid(palette.style(palette::ATTENTION))
        .context(palette.style(palette::MUTED))
        .context_value(palette.style(palette::REUSE))
}
