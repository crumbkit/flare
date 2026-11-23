//! flare's own documents: the shell with the band, the index, the launch
//! document and the stylesheet. Page content never enters these documents
//! except as the escaped `srcdoc` of a frame (`frame.rs`, threat 12).

/// The band's mark: an arched outline with a door, beside the name.
const MARK: &str = "<span class=\"mark\"><svg viewBox=\"0 0 20 20\" aria-hidden=\"true\"><path d=\"M2 17V9q8-7 16 0v8Z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\"/><path d=\"M7 17v-5h6v5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\"/></svg>flare</span>";

/// The rules for the server's own markup. flare-render's stylesheet comes
/// first and holds the tokens these rules use.
const SERVER_CSS: &str = r"
/* flare-server: the band, the mat around the frame, the index */
body.flare { margin: 0; background: var(--h-chalk); color: var(--h-ink); font-family: var(--font-ui); font-size: 15px; line-height: 1.5; }
body.flare a { color: var(--h-act); text-underline-offset: 2px; }
.mono, body.flare :not(pre) > code { font-family: var(--font-mono); font-size: 0.86em; overflow-wrap: anywhere; }
.band { background: var(--h-tint); border-bottom: 1px solid var(--h-tint-line); padding: 10px 16px; display: flex; flex-wrap: wrap; align-items: center; gap: 8px 14px; }
.mark { font-weight: 700; display: inline-flex; align-items: center; gap: 6px; }
.mark svg { width: 18px; height: 18px; flex: none; }
.where { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 8px; min-width: 0; flex: 1 1 220px; }
.where .kind { font-weight: 600; }
.where .ver { color: var(--h-muted); font-size: 0.86rem; }
.mat { padding: 12px 16px 16px; display: grid; gap: 6px; }
.cap { font-size: 0.74rem; color: var(--h-muted); display: flex; flex-wrap: wrap; justify-content: space-between; gap: 2px 12px; min-width: 0; }
.frame { width: 100%; height: 70vh; border: 1px solid var(--rule); border-radius: 4px; background: var(--paper); }
.own { padding: 20px 16px; display: grid; gap: 14px; max-width: 920px; }
.tree { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
.tree li { display: flex; flex-wrap: wrap; gap: 4px 12px; align-items: baseline; }
.tree .k { font-weight: 600; min-width: 5em; }
.tree .d1 { padding-left: 1.5em; }

/* flare-server: the reading page of a share (the mockup's artboard 01) */
body.reading { background: var(--app); color: var(--fg); font-family: var(--font-body); font-size: 14px; line-height: 1.45; }
.reading *, .reading *::before, .reading *::after { box-sizing: border-box; }
.bar { display: flex; align-items: center; justify-content: space-between; gap: 12px 20px; flex-wrap: wrap; padding: 10px 32px; border-bottom: 1px solid var(--line); background: var(--surface-2); }
.brand { font-weight: 600; font-size: 15px; color: var(--fg); }
.who { width: 28px; height: 28px; border-radius: var(--radius); background: var(--card); border: 1px solid var(--line); display: inline-flex; align-items: center; justify-content: center; font-size: 11px; font-weight: 600; color: var(--muted); flex: none; }
.visually-hidden { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
.screen { padding: 28px 32px 36px; display: flex; flex-direction: column; gap: 22px; }
.reading .doc { width: 100%; margin-inline: auto; }
.reading .doc > h1 + p { font-size: 18px; color: var(--muted); }
.share-meta { display: flex; flex-direction: column; gap: 8px; }
.share-meta .line { font-size: 13px; color: var(--muted); display: flex; gap: 6px 10px; align-items: center; flex-wrap: wrap; }
.share-meta b { color: var(--fg); font-weight: 600; }
@media (max-width: 640px) {
  .bar { padding: 10px 16px; }
  .screen { padding: 20px 16px 28px; }
}
";

/// flare-render's stylesheet followed by the server's rules.
pub(crate) fn stylesheet() -> String {
    format!("{}{SERVER_CSS}", flare_render::stylesheet())
}

/// Escapes `text` for an element's content or a quoted attribute.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\0' => out.push('\u{FFFD}'),
            other => out.push(other),
        }
    }
    out
}

/// What the band names.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Band<'a> {
    /// flare itself: the index and the server's messages.
    Flare,
    /// A page and the version shown.
    Page {
        /// The page's name.
        name: &'a str,
        /// The version shown.
        version: &'a str,
    },
    /// A binding, its page and the version it pins.
    Binding {
        /// The binding's name.
        name: &'a str,
        /// The page it pins.
        page: &'a str,
        /// The pinned version.
        version: &'a str,
    },
}

impl Band<'_> {
    fn markup(self) -> String {
        let place = match self {
            Self::Flare => String::new(),
            Self::Page { name, version } => format!(
                "<span class=\"where\"><span class=\"kind\">Page</span><span class=\"mono\">{}</span><span class=\"ver\">version {}</span></span>",
                escape(name),
                escape(version)
            ),
            Self::Binding {
                name,
                page,
                version,
            } => format!(
                "<span class=\"where\"><span class=\"kind\">Binding</span><span class=\"mono\">{}</span><span class=\"ver\">of <a href=\"/pages/{page}\">{page}</a>, pinned to version {}</span></span>",
                escape(name),
                escape(version),
                page = escape(page),
            ),
        };
        format!(
            "<div class=\"band\"><a class=\"mark-link\" href=\"/flare/\">{MARK}</a>{place}</div>\n"
        )
    }
}

/// A flare document, built part by part.
#[derive(Debug)]
pub(crate) struct Doc<'a> {
    title: &'a str,
    band: Band<'a>,
    refresh: Option<&'a str>,
    code: Option<&'a str>,
    main: String,
}

impl<'a> Doc<'a> {
    /// A document titled `title` under `band`.
    pub(crate) fn new(title: &'a str, band: Band<'a>) -> Self {
        Self {
            title,
            band,
            refresh: None,
            code: None,
            main: String::new(),
        }
    }

    /// Sends the browser on to the local path `target` at once.
    pub(crate) fn refresh(mut self, target: &'a str) -> Self {
        self.refresh = Some(target);
        self
    }

    /// Names the refusal's code on the main element.
    pub(crate) fn code(mut self, code: &'a str) -> Self {
        self.code = Some(code);
        self
    }

    /// The main element's markup, which the caller escaped.
    pub(crate) fn main(mut self, markup: String) -> Self {
        self.main = markup;
        self
    }

    /// The whole document.
    pub(crate) fn finish(self) -> String {
        let refresh = self.refresh.map_or_else(String::new, |target| {
            format!(
                "<meta http-equiv=\"refresh\" content=\"0;url={}\">\n",
                escape(target)
            )
        });
        let code = self.code.map_or_else(String::new, |code| {
            format!(" data-code=\"{}\"", escape(code))
        });
        format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{refresh}<title>{} - flare</title>\n<link rel=\"stylesheet\" href=\"/flare/style.css\">\n</head>\n<body class=\"flare\">\n{}<main{code}>\n{}\n</main>\n</body>\n</html>\n",
            escape(self.title),
            self.band.markup(),
            self.main
        )
    }
}

/// The document that `/launch` and `/auth/oidc/finish` answer with: it
/// sends the browser on to `target`, a local path, and links to it.
pub(crate) fn launch(target: &str) -> String {
    let main = format!(
        "<div class=\"own\"><p>You are signed in. <a href=\"{}\">Continue</a></p></div>",
        escape(target)
    );
    Doc::new("Signed in", Band::Flare)
        .refresh(target)
        .main(main)
        .finish()
}

/// One line of the index.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The page's name.
    pub(crate) page: String,
    /// The names of the page's bindings that the person may read.
    pub(crate) bindings: Vec<String>,
}

/// The index at `/flare/`: each page and its bindings as links.
pub(crate) fn index(entries: &[Entry]) -> String {
    let mut main = String::from("<div class=\"own\"><h1>Pages</h1>");
    if entries.is_empty() {
        main.push_str("<p>No page is published yet. Run <code>flare publish</code>.</p>");
    } else {
        main.push_str("<ul class=\"tree\">");
        for entry in entries {
            let page = escape(&entry.page);
            main.push_str(&format!(
                "<li><span class=\"k\">Page</span><a class=\"mono\" href=\"/pages/{page}\">{page}</a></li>"
            ));
            for binding in &entry.bindings {
                let binding = escape(binding);
                main.push_str(&format!(
                    "<li class=\"d1\"><span class=\"k\">Binding</span><a class=\"mono\" href=\"/b/{binding}\">{binding}</a></li>"
                ));
            }
        }
        main.push_str("</ul>");
    }
    main.push_str("</div>");
    Doc::new("Pages", Band::Flare).main(main).finish()
}
