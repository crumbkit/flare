//! Mermaid diagrams drawn as SVG on the server with limu, so the reading
//! page shows them with no script.
//!
//! [`draw`] lays a diagram out for Flare's 820 px column with 14 px text
//! measured as IBM Plex Sans, and writes it with its colours as `--limu-*`
//! CSS variables. limu draws no link and no image, because both are off.
//! Flowcharts, graphs and state diagrams take right-angle edges, which keep
//! an edge clear of a subgraph's border; class and ER diagrams keep limu's
//! own edges, because right angles stack their arrowheads and crowd their
//! markers. A source over [`MAX_SOURCE_BYTES`] is not laid out.

mod plex;
mod theme;

use limu::layout::{Options, Routing};
use limu::scene::svg;

/// The longest source [`draw`] lays out: 50,000 bytes, Mermaid's own limit
/// on a diagram's text. A longer source shows as text with a note that it
/// is too long to draw, which bounds the work one diagram can ask of the
/// server.
pub(crate) const MAX_SOURCE_BYTES: usize = 50_000;

/// The size of text with no size of its own: node labels and titles.
const TEXT_PX: f64 = 14.0;

/// The width of the reading page's `.doc` column, which limu fits a
/// diagram to where it can.
const COLUMN_PX: f64 = 820.0;

/// The smallest size the drawing's smallest text may shrink to when the
/// drawing is scaled to fit its box. A wider drawing keeps this size and
/// scrolls in its box.
const SMALLEST_TEXT_PX: f64 = 10.5;

/// A Mermaid diagram type a page may draw, named by the first word of its
/// source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `flowchart`.
    Flowchart,
    /// `graph`, the older name of a flowchart.
    Graph,
    /// `sequenceDiagram`.
    Sequence,
    /// `stateDiagram`.
    State,
    /// `classDiagram`.
    Class,
    /// `erDiagram`.
    Er,
    /// `gantt`.
    Gantt,
    /// `pie`.
    Pie,
}

impl Kind {
    /// Every type a page may draw.
    pub(crate) const ALL: [Kind; 8] = [
        Kind::Flowchart,
        Kind::Graph,
        Kind::Sequence,
        Kind::State,
        Kind::Class,
        Kind::Er,
        Kind::Gantt,
        Kind::Pie,
    ];

    /// The word that starts a source of this type.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Kind::Flowchart => "flowchart",
            Kind::Graph => "graph",
            Kind::Sequence => "sequenceDiagram",
            Kind::State => "stateDiagram",
            Kind::Class => "classDiagram",
            Kind::Er => "erDiagram",
            Kind::Gantt => "gantt",
            Kind::Pie => "pie",
        }
    }

    /// The type whose word is the first word of `source`, if any.
    pub(crate) fn of(source: &str) -> Option<Kind> {
        let first = source.split_whitespace().next()?;
        Kind::ALL.into_iter().find(|kind| kind.word() == first)
    }

    /// How the edges of a diagram of this type run. Sequence diagrams,
    /// gantt charts and pies have no ranked edges, so limu ignores it.
    pub(crate) fn routing(self) -> Routing {
        match self {
            Kind::Flowchart | Kind::Graph | Kind::State => Routing::Orthogonal,
            Kind::Sequence | Kind::Class | Kind::Er | Kind::Gantt | Kind::Pie => Routing::Auto,
        }
    }
}

/// What [`draw`] made of a diagram's source.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Drawing {
    /// limu drew it.
    Drawn {
        /// The `<svg>` element.
        svg: String,
        /// The narrowest the drawing's box may be, in CSS pixels, so its
        /// smallest text shows at 10.5 px or more.
        min_width: f64,
        /// The drawing's accessible name from its source: its accDescr,
        /// or its accTitle when it has no accDescr.
        name: Option<String>,
    },
    /// limu could not parse or lay out the source; the text is limu's
    /// message.
    Refused(String),
    /// The source is over [`MAX_SOURCE_BYTES`], so it was not laid out.
    TooLong,
}

/// Draws `source`, a diagram of type `kind`, as SVG.
pub(crate) fn draw(kind: Kind, source: &str) -> Drawing {
    if source.len() > MAX_SOURCE_BYTES {
        return Drawing::TooLong;
    }
    let options = Options {
        measurer: &plex::Plex,
        font_size: TEXT_PX,
        routing: kind.routing(),
        links: false,
        images: false,
        max_width: COLUMN_PX,
        keep_direction: true,
        ..Options::default()
    };
    match limu::layout(source, &options) {
        Ok(mut scene) => {
            // The page names the drawing's box, which is role="img" and so
            // hides the SVG's own <title> and <desc>. limu leaves them out
            // when the scene has none, so the SVG is the same with or
            // without accTitle and accDescr.
            let title = scene.title.take();
            let name = scene.desc.take().or(title);
            let svg = svg::render(&scene, &theme::options());
            let min_width = min_width(scene.width, smallest_text(&svg));
            Drawing::Drawn {
                svg,
                min_width,
                name,
            }
        }
        Err(error) => Drawing::Refused(error.to_string()),
    }
}

/// The narrowest a drawing `width` px wide may be shown, so text
/// `smallest` px high shows at [`SMALLEST_TEXT_PX`] or more. A drawing
/// whose text is already smaller is never shown wider than itself.
fn min_width(width: f64, smallest: f64) -> f64 {
    (width * SMALLEST_TEXT_PX / smallest).min(width).ceil()
}

/// The smallest `font-size` in px that `svg` sets, or [`TEXT_PX`] when it
/// sets none smaller.
///
/// This reads only the numeric `font-size` attributes limu writes: the
/// drawing's own size and the smaller size of secondary labels. A relative
/// size such as `smaller`, which limu writes for a subscript, and a size in
/// an author's `style` line are not counted, so such text can shrink below
/// 10.5 px.
fn smallest_text(svg: &str) -> f64 {
    svg.split(" font-size=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter_map(|value| value.parse::<f64>().ok())
        .filter(|size| *size > 0.0)
        .fold(TEXT_PX, f64::min)
}

#[cfg(test)]
mod tests;
