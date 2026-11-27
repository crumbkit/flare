//! The look limu writes a drawing in. limu writes each colour of the theme
//! as a `--limu-*` CSS variable with the theme's colour as its fallback, and
//! flare.css sets every variable from Flare's tokens in both colour schemes,
//! so one drawing serves light and dark. The fallbacks are Flare's light
//! tokens as hex, which a page without flare's stylesheet shows.

use limu::scene::style::{Color, Look, Theme};
use limu::scene::svg;

/// The font the drawing was measured in, after the page's `--limu-font`.
const FONT: &str = "\"IBM Plex Sans\", system-ui, sans-serif";

/// Flare's light tokens as hex. The series are the light scheme's eight
/// series colours in flare.css, each of which meets 4.5:1 against the
/// background, because limu writes a pie slice's label in the background
/// colour on the slice.
const FLARE: Theme = Theme {
    bg: Color::hex("#f2f6f8"),
    fg: Color::hex("#0c1318"),
    muted: Color::hex("#464e55"),
    line: Color::hex("#747b81"),
    surface: Color::hex("#fcfeff"),
    surface_alt: Color::hex("#edf1f3"),
    border: Color::hex("#747b81"),
    accent: Color::hex("#00639a"),
    critical: Color::hex("#b3261e"),
    series: [
        Color::hex("#00639a"),
        Color::hex("#0b5d2a"),
        Color::hex("#7d460b"),
        Color::hex("#9a2929"),
        Color::hex("#5f3d8e"),
        Color::hex("#135f5b"),
        Color::hex("#464e55"),
        Color::hex("#1b3b71"),
    ],
    ..Theme::PELE
};

/// How limu writes a drawing: colours as variables, never as literal
/// colours, and text in IBM Plex Sans.
pub(super) fn options() -> svg::Options<'static> {
    svg::Options {
        look: Look::new(FLARE),
        literal: false,
        font_family: Some(FONT.to_owned()),
        ..svg::Options::default()
    }
}
