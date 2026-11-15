//! Markdown pages, rendered with raw HTML shown as text (threat 15).
//!
//! A Markdown page is not a template: `{{` stays text, because filling
//! first would let a record value add Markdown links.

use crate::html::escape_into;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Renders Markdown into HTML. Raw HTML shows as escaped text, a link keeps
/// its target only for `http`, `https`, `mailto` and relative targets, and an
/// image keeps only a relative source.
pub(crate) fn to_html(markdown: &str) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut writer = Writer::default();
    for event in Parser::new_ext(markdown, options) {
        writer.event(event);
    }
    writer.out
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
    let head = target.split(['/', '?', '#']).next().unwrap_or_default();
    plain(target) && !head.contains(':') && !target.starts_with("//")
}

/// Whether `target` holds no whitespace, control character or backslash,
/// which a browser strips or reads as `/` before it finds the scheme.
fn plain(target: &str) -> bool {
    !target
        .chars()
        .any(|character| character.is_whitespace() || character.is_control() || character == '\\')
}

/// Writes HTML for Markdown events.
#[derive(Debug, Default)]
struct Writer {
    out: String,
    /// For each open link, whether it was written as `<a>`.
    links: Vec<bool>,
    /// The image being read: its source when it keeps one, and its
    /// alternative text so far.
    image: Option<(Option<String>, String)>,
    /// The alignment of each column of the table being read.
    alignments: Vec<Alignment>,
    cell: usize,
    in_head: bool,
}

impl Writer {
    fn text(&mut self, text: &str) {
        match &mut self.image {
            Some((_, alternative)) => alternative.push_str(text),
            None => escape_into(&mut self.out, text),
        }
    }

    fn event(&mut self, event: Event<'_>) {
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

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.out.push_str("<p>"),
            Tag::Heading { level, .. } => {
                self.out.push('<');
                self.out.push_str(&level.to_string());
                self.out.push('>');
            }
            Tag::BlockQuote(_) => self.out.push_str("<blockquote>\n"),
            Tag::CodeBlock(kind) => {
                self.out.push_str("<pre><code");
                if let CodeBlockKind::Fenced(info) = kind
                    && let Some(language) = info.split_whitespace().next()
                {
                    self.out.push_str(" class=\"language-");
                    escape_into(&mut self.out, language);
                    self.out.push('"');
                }
                self.out.push('>');
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
                self.out.push_str("<table>\n");
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
                dest_url, title, ..
            } => {
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
            TagEnd::CodeBlock => self.out.push_str("</code></pre>\n"),
            TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
            TagEnd::List(true) => self.out.push_str("</ol>\n"),
            TagEnd::List(false) => self.out.push_str("</ul>\n"),
            TagEnd::Item => self.out.push_str("</li>\n"),
            TagEnd::FootnoteDefinition => self.out.push_str("</div>\n"),
            TagEnd::DefinitionList => self.out.push_str("</dl>\n"),
            TagEnd::DefinitionListTitle => self.out.push_str("</dt>\n"),
            TagEnd::DefinitionListDefinition => self.out.push_str("</dd>\n"),
            TagEnd::Table => self.out.push_str("</tbody>\n</table>\n"),
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
