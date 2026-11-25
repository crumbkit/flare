//! Markdown pages, rendered with raw HTML shown as text (threat 15).
//!
//! A Markdown page is not a template: `{{` stays text, because filling
//! first would let a record value add Markdown links.
//!
//! A fenced block whose language is `mermaid` and whose first word is one of
//! [`DIAGRAM_TYPES`] becomes a `<figure class="diagram">` that holds its
//! escaped source; a script draws it. Every other code block becomes a
//! `<div class="code">` with its language label and a hidden Copy button that
//! a script shows. A paragraph that starts with "Figure", a number and a full
//! stop, written directly after a top-level diagram, table or code block,
//! becomes that block's `<figcaption>`.

mod blocks;
mod images;

pub use images::{ImageSources, image_paths, unsendable_image_paths};

use blocks::{captionable, diagram_type, is_caption, top_level_blocks};

use crate::html::escape_into;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

/// The Mermaid diagram types a page may draw. A Mermaid fence whose source
/// starts with any other word, such as a `%%{init}%%` directive or front
/// matter, renders as code. A directive on a later line still reaches
/// Mermaid; the page is safe because Mermaid keeps `securityLevel` among its
/// secure keys, which no directive can change.
pub(crate) const DIAGRAM_TYPES: [&str; 8] = [
    "flowchart",
    "graph",
    "sequenceDiagram",
    "stateDiagram",
    "classDiagram",
    "erDiagram",
    "gantt",
    "pie",
];

/// The label of a code block that names no language.
const PLAIN: &str = "text";

/// Rendered Markdown, and what scripts it has work for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rendered {
    /// The HTML.
    pub(crate) html: String,
    /// Whether it holds a code block that a highlighter colours.
    pub(crate) code: bool,
    /// Whether it holds a diagram that Mermaid draws.
    pub(crate) diagrams: bool,
}

/// Renders Markdown into HTML. Raw HTML shows as escaped text, a link keeps
/// its target only for `http`, `https`, `mailto` and relative targets, and an
/// image keeps only a relative source.
pub(crate) fn to_html(markdown: &str) -> String {
    render(markdown).html
}

/// Renders Markdown as [`to_html`] does, and says whether the HTML holds
/// code or a diagram.
pub(crate) fn render(markdown: &str) -> Rendered {
    render_with(markdown, None)
}

/// The parser options every rendering uses.
const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS);

/// Renders as [`render`] does. With `sources`, a relative image shows from
/// the URL `sources` gives for its path, and as its alternative text when
/// `sources` has none.
pub(crate) fn render_with(markdown: &str, sources: Option<&ImageSources>) -> Rendered {
    let events: Vec<Event<'_>> = Parser::new_ext(markdown, OPTIONS).collect();
    let blocks = top_level_blocks(&events);
    let mut writer = Writer {
        sources: sources.cloned(),
        ..Writer::default()
    };
    let mut index = 0;
    while let Some(&(start, end)) = blocks.get(index) {
        let block = events.get(start..end).unwrap_or_default();
        let caption = blocks
            .get(index + 1)
            .and_then(|&(start, end)| events.get(start..end))
            .filter(|next| captionable(block) && is_caption(next));
        match caption {
            Some(caption) => {
                // The caption is the paragraph's inline events, without
                // its own start and end.
                let inline = caption
                    .get(1..caption.len().saturating_sub(1))
                    .unwrap_or_default();
                writer.caption = Some(inline.to_vec());
                index += 2;
            }
            None => index += 1,
        }
        for event in block {
            writer.event(event.clone());
        }
        writer.caption = None;
    }
    Rendered {
        html: writer.out,
        code: writer.code,
        diagrams: writer.diagrams,
    }
}

/// Whether a link may keep `target`: `http`, `https` and `mailto` targets,
/// and relative targets that a browser cannot read as another scheme or
/// host.
fn link_allowed(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    let absolute = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme));
    plain(target) && (absolute || relative(target))
}

/// Whether `target` is a relative reference: no scheme before its first
/// `/`, `?` or `#`, and no leading `//`.
fn relative(target: &str) -> bool {
    plain(target) && images::local(target)
}

/// Whether `target` holds no whitespace, control character or backslash,
/// which a browser strips or reads as `/` before it finds the scheme.
fn plain(target: &str) -> bool {
    !target
        .chars()
        .any(|character| character.is_whitespace() || character.is_control() || character == '\\')
}

/// A code block being read: its language and its text so far.
#[derive(Debug)]
struct CodeBlock {
    language: Option<String>,
    text: String,
}

/// Writes HTML for Markdown events.
#[derive(Debug, Default)]
struct Writer<'a> {
    out: String,
    /// The caption of the top-level block being written.
    caption: Option<Vec<Event<'a>>>,
    /// The code block being read.
    block: Option<CodeBlock>,
    /// Whether a code block was written.
    code: bool,
    /// Whether a diagram was written.
    diagrams: bool,
    /// For each open link, whether it was written as `<a>`.
    links: Vec<bool>,
    /// The image being read: its source when it keeps one, and its
    /// alternative text so far.
    image: Option<(Option<String>, String)>,
    /// Where a share's listed images load from, when rendering a share.
    sources: Option<ImageSources>,
    /// The alignment of each column of the table being read.
    alignments: Vec<Alignment>,
    cell: usize,
    in_head: bool,
}

impl<'a> Writer<'a> {
    fn text(&mut self, text: &str) {
        if let Some(block) = &mut self.block {
            block.text.push_str(text);
            return;
        }
        match &mut self.image {
            Some((_, alternative)) => alternative.push_str(text),
            None => escape_into(&mut self.out, text),
        }
    }

    fn event(&mut self, event: Event<'a>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => self.text(&text),
            Event::InlineMath(text) | Event::DisplayMath(text) => self.text(&text),
            Event::Code(code) => {
                if self.image.is_some() {
                    self.text(&code);
                } else {
                    self.out.push_str("<code>");
                    escape_into(&mut self.out, &code);
                    self.out.push_str("</code>");
                }
            }
            Event::FootnoteReference(label) => self.text(&format!("[^{label}]")),
            Event::SoftBreak => self.text("\n"),
            Event::HardBreak => {
                if self.image.is_some() {
                    self.text(" ");
                } else {
                    self.out.push_str("<br>\n");
                }
            }
            Event::Rule => self.out.push_str("<hr>\n"),
            Event::TaskListMarker(done) => {
                self.out
                    .push_str(if done { "\u{2611} " } else { "\u{2610} " });
            }
        }
    }

    fn start(&mut self, tag: Tag<'a>) {
        match tag {
            Tag::Paragraph => self.out.push_str("<p>"),
            Tag::Heading { level, .. } => {
                self.out.push('<');
                self.out.push_str(&level.to_string());
                self.out.push('>');
            }
            Tag::BlockQuote(_) => self.out.push_str("<blockquote>\n"),
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().map(str::to_owned)
                    }
                    CodeBlockKind::Indented => None,
                };
                self.block = Some(CodeBlock {
                    language,
                    text: String::new(),
                });
            }
            Tag::HtmlBlock | Tag::MetadataBlock(_) => {}
            Tag::List(Some(1)) => self.out.push_str("<ol>\n"),
            Tag::List(Some(start)) => {
                self.out.push_str("<ol start=\"");
                self.out.push_str(&start.to_string());
                self.out.push_str("\">\n");
            }
            Tag::List(None) => self.out.push_str("<ul>\n"),
            Tag::Item => self.out.push_str("<li>"),
            Tag::FootnoteDefinition(_) => self.out.push_str("<div class=\"footnote\">"),
            Tag::DefinitionList => self.out.push_str("<dl>\n"),
            Tag::DefinitionListTitle => self.out.push_str("<dt>"),
            Tag::DefinitionListDefinition => self.out.push_str("<dd>"),
            Tag::Table(alignments) => {
                self.alignments = alignments;
                if self.caption.is_some() {
                    self.out.push_str("<figure class=\"table\">\n");
                }
                // The box takes focus, so a keyboard can scroll a wide table.
                self.out
                    .push_str("<div class=\"scroll\" tabindex=\"0\">\n<table>\n");
            }
            Tag::TableHead => {
                self.in_head = true;
                self.cell = 0;
                self.out.push_str("<thead>\n<tr>");
            }
            Tag::TableRow => {
                self.cell = 0;
                self.out.push_str("<tr>");
            }
            Tag::TableCell => self.cell_start(),
            Tag::Emphasis => self.out.push_str("<em>"),
            Tag::Strong => self.out.push_str("<strong>"),
            Tag::Strikethrough => self.out.push_str("<del>"),
            Tag::Superscript => self.out.push_str("<sup>"),
            Tag::Subscript => self.out.push_str("<sub>"),
            Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            } => {
                // An email autolink such as <someone@example.com> gives its
                // address with no scheme.
                let dest_url = if link_type == LinkType::Email {
                    format!("mailto:{dest_url}").into()
                } else {
                    dest_url
                };
                let keep = self.image.is_none() && link_allowed(&dest_url);
                if keep {
                    self.out.push_str("<a href=\"");
                    escape_into(&mut self.out, &dest_url);
                    if !title.is_empty() {
                        self.out.push_str("\" title=\"");
                        escape_into(&mut self.out, &title);
                    }
                    self.out.push_str("\">");
                }
                self.links.push(keep);
            }
            Tag::Image { dest_url, .. } => {
                let source = relative(&dest_url).then(|| dest_url.to_string());
                let source = match &self.sources {
                    Some(sources) => source.and_then(|path| sources.get(&path).map(str::to_owned)),
                    None => source,
                };
                self.image = Some((source, String::new()));
            }
        }
    }

    fn cell_start(&mut self) {
        let element = if self.in_head { "<th" } else { "<td" };
        self.out.push_str(element);
        let class = match self.alignments.get(self.cell) {
            Some(Alignment::Left) => " class=\"align-left\"",
            Some(Alignment::Center) => " class=\"align-center\"",
            Some(Alignment::Right) => " class=\"align-right\"",
            Some(Alignment::None) | None => "",
        };
        self.out.push_str(class);
        self.out.push('>');
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.out.push_str("</p>\n"),
            TagEnd::Heading(level) => {
                self.out.push_str("</");
                self.out.push_str(&level.to_string());
                self.out.push_str(">\n");
            }
            TagEnd::BlockQuote(_) => self.out.push_str("</blockquote>\n"),
            TagEnd::CodeBlock => self.code_end(),
            TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
            TagEnd::List(true) => self.out.push_str("</ol>\n"),
            TagEnd::List(false) => self.out.push_str("</ul>\n"),
            TagEnd::Item => self.out.push_str("</li>\n"),
            TagEnd::FootnoteDefinition => self.out.push_str("</div>\n"),
            TagEnd::DefinitionList => self.out.push_str("</dl>\n"),
            TagEnd::DefinitionListTitle => self.out.push_str("</dt>\n"),
            TagEnd::DefinitionListDefinition => self.out.push_str("</dd>\n"),
            TagEnd::Table => {
                self.out.push_str("</tbody>\n</table>\n</div>\n");
                if self.caption.is_some() {
                    self.close_figure();
                }
            }
            TagEnd::TableHead => {
                self.in_head = false;
                self.out.push_str("</tr>\n</thead>\n<tbody>\n");
            }
            TagEnd::TableRow => self.out.push_str("</tr>\n"),
            TagEnd::TableCell => {
                self.out
                    .push_str(if self.in_head { "</th>" } else { "</td>" });
                self.cell += 1;
            }
            TagEnd::Emphasis => self.out.push_str("</em>"),
            TagEnd::Strong => self.out.push_str("</strong>"),
            TagEnd::Strikethrough => self.out.push_str("</del>"),
            TagEnd::Superscript => self.out.push_str("</sup>"),
            TagEnd::Subscript => self.out.push_str("</sub>"),
            TagEnd::Link => {
                if self.links.pop() == Some(true) {
                    self.out.push_str("</a>");
                }
            }
            TagEnd::Image => self.image_end(),
        }
    }

    /// Writes the code block just read as a diagram or as code.
    fn code_end(&mut self) {
        let Some(block) = self.block.take() else {
            return;
        };
        let language = block.language.as_deref().unwrap_or(PLAIN);
        if let Some(kind) = diagram_type(language, &block.text) {
            self.diagrams = true;
            self.out.push_str(
                "<figure class=\"diagram\">\n<pre class=\"diagram-source\" data-diagram=\"",
            );
            self.out.push_str(kind);
            self.out.push_str("\">");
            escape_into(&mut self.out, &block.text);
            self.out.push_str("</pre>\n");
            self.close_figure();
            return;
        }
        self.code = true;
        if self.caption.is_some() {
            self.out.push_str("<figure class=\"listing\">\n");
        }
        self.out
            .push_str("<div class=\"code\">\n<div class=\"code-head\"><span class=\"mono\">");
        escape_into(&mut self.out, language);
        self.out.push_str(
            "</span><button class=\"text-btn\" type=\"button\" data-copy hidden>Copy</button></div>\n<pre><code class=\"language-",
        );
        escape_into(&mut self.out, language);
        self.out.push_str("\" data-lang=\"");
        escape_into(&mut self.out, language);
        self.out.push_str("\">");
        escape_into(&mut self.out, &block.text);
        self.out.push_str("</code></pre>\n</div>\n");
        if self.caption.is_some() {
            self.close_figure();
        }
    }

    /// Writes the caption, when the block has one, and closes its figure.
    fn close_figure(&mut self) {
        if let Some(caption) = self.caption.take() {
            self.out.push_str("<figcaption>");
            for event in caption {
                self.event(event);
            }
            self.out.push_str("</figcaption>\n");
        }
        self.out.push_str("</figure>\n");
    }

    fn image_end(&mut self) {
        let Some((source, alternative)) = self.image.take() else {
            return;
        };
        match source {
            Some(source) => {
                self.out.push_str("<img src=\"");
                escape_into(&mut self.out, &source);
                self.out.push_str("\" alt=\"");
                escape_into(&mut self.out, &alternative);
                self.out.push_str("\">");
            }
            None => escape_into(&mut self.out, &alternative),
        }
    }
}

#[cfg(test)]
mod tests;
