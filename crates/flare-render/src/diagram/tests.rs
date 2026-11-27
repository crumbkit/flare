//! Unit tests of drawing diagrams with limu, on the eleven sources of
//! /evals/flare-diagram-renderers, and of the stylesheet's limu variables.

use super::*;
use limu::scene::text::{Face, Measurer};

/// The eval's eleven sources, with the type a page draws each as. The
/// state diagram's fixture starts `stateDiagram-v2`, which limu reads as a
/// state diagram.
const FIXTURES: [(&str, Kind, &str); 11] = [
    (
        "01-figure-1",
        Kind::Flowchart,
        include_str!("../../tests/diagrams/01-figure-1.mmd"),
    ),
    (
        "02-subgraphs-cross",
        Kind::Flowchart,
        include_str!("../../tests/diagrams/02-subgraphs-cross.mmd"),
    ),
    (
        "03-crate-deps",
        Kind::Flowchart,
        include_str!("../../tests/diagrams/03-crate-deps.mmd"),
    ),
    (
        "04-module-deps",
        Kind::Flowchart,
        include_str!("../../tests/diagrams/04-module-deps.mmd"),
    ),
    (
        "05-sequence",
        Kind::Sequence,
        include_str!("../../tests/diagrams/05-sequence.mmd"),
    ),
    (
        "06-state",
        Kind::State,
        include_str!("../../tests/diagrams/06-state.mmd"),
    ),
    (
        "07-class",
        Kind::Class,
        include_str!("../../tests/diagrams/07-class.mmd"),
    ),
    (
        "08-er",
        Kind::Er,
        include_str!("../../tests/diagrams/08-er.mmd"),
    ),
    (
        "09-gantt",
        Kind::Gantt,
        include_str!("../../tests/diagrams/09-gantt.mmd"),
    ),
    (
        "10-pie",
        Kind::Pie,
        include_str!("../../tests/diagrams/10-pie.mmd"),
    ),
    (
        "11-graph-tiny",
        Kind::Graph,
        include_str!("../../tests/diagrams/11-graph-tiny.mmd"),
    ),
];

/// The SVG of `source` drawn as `kind`, failing the test when limu does
/// not draw it.
fn svg_of(kind: Kind, source: &str) -> (String, f64) {
    match draw(kind, source) {
        Drawing::Drawn { svg, min_width } => (svg, min_width),
        other => panic!("not drawn: {other:?}"),
    }
}

/// Whether a tag in `svg` holds an attribute whose name starts with `on`,
/// which a browser runs as an event handler. limu escapes `<` in text, so
/// each `<` opens a tag.
fn has_handler(svg: &str) -> bool {
    svg.split('<')
        .filter_map(|rest| rest.split('>').next())
        .any(|tag| {
            tag.match_indices(" on").any(|(at, _)| {
                let rest = tag.get(at + 3..).unwrap_or_default();
                let name = rest.chars().take_while(char::is_ascii_alphanumeric).count();
                name > 0 && rest.get(name..).is_some_and(|tail| tail.starts_with('='))
            })
        })
}

/// Asserts that `svg` is one `<svg>` that runs nothing and loads nothing.
fn assert_inert(name: &str, svg: &str) {
    assert!(svg.starts_with("<svg "), "{name}: {svg}");
    assert_eq!(svg.matches("<svg").count(), 1, "{name}");
    for refused in ["<script", "href", "<foreignObject", "<image", "<a ", "url("] {
        assert!(!svg.contains(refused), "{name} holds {refused}: {svg}");
    }
    assert!(!has_handler(svg), "{name}: {svg}");
}

#[test]
fn each_of_the_eleven_eval_sources_draws_one_inert_svg() {
    for (name, kind, source) in FIXTURES {
        let (svg, min_width) = svg_of(kind, source);
        assert_inert(name, &svg);
        assert!(min_width > 0.0, "{name}");
    }
}

#[test]
fn a_hostile_label_draws_as_escaped_text_in_an_inert_svg() {
    let source = "flowchart LR\n  a[\"<img src=x onerror=alert(1)>\"] --> b[\"</pre><script>alert(1)</script>\"]\n  c[\"<a href=javascript:alert(1)>x</a>\"] --> a\n  click a \"javascript:alert(1)\"\n";
    let (svg, _) = svg_of(Kind::Flowchart, source);
    assert_inert("hostile", &svg);
    // limu reads a label as Mermaid's HTML labels are read: it drops the
    // tags and keeps their text.
    assert!(!svg.contains("<img"), "{svg}");
    assert!(svg.contains(">alert(1)</tspan>"), "{svg}");
}

#[test]
fn figure_1_draws_exactly_as_the_signed_mockup_shows_it() {
    let (svg, min_width) = svg_of(Kind::Flowchart, FIXTURES[0].2);
    assert_eq!(svg, include_str!("../../tests/diagrams/01-figure-1.svg"));
    // The mockup's box is 816 px at its narrowest: 932 px of drawing whose
    // 12 px edge labels may shrink to 10.5 px.
    assert!((min_width - 816.0).abs() < f64::EPSILON, "{min_width}");
}

#[test]
fn flowcharts_graphs_and_state_diagrams_route_at_right_angles_and_class_and_er_diagrams_do_not() {
    for kind in [Kind::Flowchart, Kind::Graph, Kind::State] {
        assert_eq!(kind.routing(), Routing::Orthogonal, "{kind:?}");
    }
    for kind in [Kind::Class, Kind::Er] {
        assert_eq!(kind.routing(), Routing::Auto, "{kind:?}");
    }
    // Under right angles an edge is straight runs and rounded corners, with
    // no cubic curve.
    let (svg, _) = svg_of(Kind::Flowchart, FIXTURES[0].2);
    let edges: Vec<&str> = svg
        .split("<g class=\"limu-edge")
        .skip(1)
        .filter_map(|edge| edge.split(" d=\"").nth(1))
        .filter_map(|data| data.split('"').next())
        .collect();
    assert!(!edges.is_empty(), "{svg}");
    for data in edges {
        assert!(!data.contains('C'), "{data}");
    }
}

/// A flowchart of exactly `bytes` bytes, padded with a comment line.
fn flowchart_of(bytes: usize) -> String {
    let head = "flowchart LR\n  a --> b\n%% ";
    let mut source = String::from(head);
    source.push_str(&"x".repeat(bytes - head.len() - 1));
    source.push('\n');
    assert_eq!(source.len(), bytes);
    source
}

#[test]
fn a_source_of_50_000_bytes_draws_and_one_of_50_001_is_not_laid_out() {
    assert!(matches!(
        draw(Kind::Flowchart, &flowchart_of(MAX_SOURCE_BYTES)),
        Drawing::Drawn { .. }
    ));
    assert_eq!(
        draw(Kind::Flowchart, &flowchart_of(MAX_SOURCE_BYTES + 1)),
        Drawing::TooLong
    );
}

#[test]
fn a_source_limu_cannot_parse_is_refused_with_limus_message() {
    match draw(Kind::Flowchart, "flowchart LR\n  a --> --> b\n  ((\n") {
        Drawing::Refused(message) => assert!(!message.is_empty()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_box_keeps_the_smallest_text_at_10_5_px_and_is_never_wider_than_the_drawing() {
    assert!((min_width(932.0, 12.0) - 816.0).abs() < f64::EPSILON);
    assert!((min_width(400.0, 14.0) - 300.0).abs() < f64::EPSILON);
    assert!((min_width(400.0, 8.0) - 400.0).abs() < f64::EPSILON);
    assert!(
        (smallest_text(
            "<svg font-size=\"14\"><text font-size=\"12\"/><text font-size=\"smaller\"/></svg>"
        ) - 12.0)
            .abs()
            < f64::EPSILON
    );
    assert!((smallest_text("<svg></svg>") - TEXT_PX).abs() < f64::EPSILON);
}

#[test]
fn plex_gives_each_ascii_character_its_own_width_and_every_other_character_one() {
    let size = 1000.0;
    assert!((plex::Plex.width("i", size, Face::REGULAR) - 250.0).abs() < f64::EPSILON);
    assert!((plex::Plex.width("W", size, Face::REGULAR) - 891.0).abs() < f64::EPSILON);
    assert!((plex::Plex.width("W", size, Face::BOLD) - 973.0).abs() < f64::EPSILON);
    assert!((plex::Plex.width("é漢", size, Face::REGULAR) - 1098.0).abs() < f64::EPSILON);
    assert!((plex::Plex.width("\n\t", size, Face::REGULAR)).abs() < f64::EPSILON);
    assert!((plex::Plex.width("ii", 14.0, Face::REGULAR) - 7.0).abs() < 1e-9);
}

#[test]
fn a_kind_is_named_by_the_first_word_of_its_source() {
    for kind in Kind::ALL {
        assert_eq!(Kind::of(&format!("{}\n  a --> b", kind.word())), Some(kind));
    }
    assert_eq!(Kind::of("journey\n  title x"), None);
    assert_eq!(Kind::of("Flowchart LR"), None);
    assert_eq!(Kind::of(""), None);
}

/// The declarations of the first rule in `css` that opens with `opener`.
fn block<'a>(css: &'a str, opener: &str) -> &'a str {
    let (_, rest) = css.split_once(opener).unwrap();
    rest.split_once('}').unwrap().0
}

/// The value `block` gives `property`, without its trailing semicolon.
fn value<'a>(block: &'a str, property: &str) -> Option<&'a str> {
    block.split(';').find_map(|declaration| {
        // A comment may come before the name.
        let declaration = declaration
            .rsplit_once("*/")
            .map_or(declaration, |(_, after)| after);
        let (name, value) = declaration.split_once(':')?;
        (name.trim() == property).then(|| value.trim())
    })
}

/// The light and the dark `:root` blocks of the stylesheet.
fn schemes() -> [(&'static str, &'static str); 2] {
    let css = crate::stylesheet();
    let (_, dark) = css
        .split_once("@media (prefers-color-scheme: dark)")
        .unwrap();
    [
        ("light", block(css, ":root {")),
        ("dark", block(dark, ":root {")),
    ]
}

/// The token a `.dg` limu variable takes: `var(--token)` gives `--token`.
fn token_of(variable: &str) -> &'static str {
    let rule = block(crate::stylesheet(), ".dg {");
    let value = value(rule, variable).unwrap_or_else(|| panic!("{variable} is not set in .dg"));
    value
        .strip_prefix("var(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{variable} is {value}, not a token"))
}

#[test]
fn every_limu_variable_the_drawings_use_has_a_value_in_both_colour_schemes() {
    let mut variables: Vec<String> = Vec::new();
    for (_, kind, source) in FIXTURES {
        let (svg, _) = svg_of(kind, source);
        for (at, _) in svg.match_indices("var(--limu-") {
            let name: String = svg
                .get(at + 4..)
                .unwrap_or_default()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if !variables.contains(&name) {
                variables.push(name);
            }
        }
    }
    assert!(
        variables.contains(&"--limu-series-1".to_owned()),
        "{variables:?}"
    );
    // A pie with more slices uses all eight series colours.
    for series in 1..=8 {
        let name = format!("--limu-series-{series}");
        if !variables.contains(&name) {
            variables.push(name);
        }
    }
    // The dark scheme inherits a token it does not set, such as the font,
    // so each token is set in the light scheme and each colour in both.
    let [(_, light), (_, dark)] = schemes();
    for variable in &variables {
        let token = token_of(variable);
        assert!(
            value(light, token).is_some(),
            "{variable} takes {token}, which is not set"
        );
        if variable != "--limu-font" {
            assert!(
                value(dark, token).is_some(),
                "{variable} takes {token}, which the dark scheme does not set"
            );
        }
    }
}

/// The relative luminance of an `oklch(L% C H)` colour, clamped into sRGB.
fn luminance(colour: &str) -> f64 {
    let inner = colour
        .strip_prefix("oklch(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{colour} is not oklch"));
    let parts: Vec<f64> = inner
        .split_whitespace()
        .map(|part| part.trim_end_matches('%').parse().unwrap())
        .collect();
    let (lightness, chroma, hue) = (parts[0] / 100.0, parts[1], parts[2].to_radians());
    let (a, b) = (chroma * hue.cos(), chroma * hue.sin());
    let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    let red = (4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s).clamp(0.0, 1.0);
    let green = (-1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s).clamp(0.0, 1.0);
    let blue = (-0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s).clamp(0.0, 1.0);
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

#[test]
fn each_series_colour_meets_4_5_to_1_against_the_background_in_each_scheme() {
    let background = token_of("--limu-bg");
    for (scheme, root) in schemes() {
        let behind = luminance(value(root, background).unwrap());
        for series in 1..=8 {
            let token = token_of(&format!("--limu-series-{series}"));
            let colour = value(root, token).unwrap();
            let front = luminance(colour);
            let ratio = (front.max(behind) + 0.05) / (front.min(behind) + 0.05);
            assert!(
                ratio >= 4.5,
                "{scheme} {token} {colour} is {ratio:.2}:1 against {background}"
            );
        }
    }
}
