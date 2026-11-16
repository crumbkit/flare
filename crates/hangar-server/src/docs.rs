//! hangar's own documents: the shell with the band, the index, the launch
//! document and the stylesheet. Page content never enters these documents
//! except as the escaped `srcdoc` of a frame (`frame.rs`, threat 12).

/// The band's mark: a hangar outline.
const MARK: &str = "<span class=\"mark\"><svg viewBox=\"0 0 20 20\" aria-hidden=\"true\"><path d=\"M2 17V9q8-7 16 0v8Z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\"/><path d=\"M7 17v-5h6v5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\"/></svg>hangar</span>";

/// The rules for the server's own markup. hangar-render's stylesheet comes
/// first and holds the tokens these rules use.
const SERVER_CSS: &str = r"
/* hangar-server: the band, the mat around the frame, the index */
body.hangar { margin: 0; background: var(--h-chalk); color: var(--h-ink); font-family: var(--font-ui); font-size: 15px; line-height: 1.5; }
body.hangar a { color: var(--h-act); text-underline-offset: 2px; }
.mono, body.hangar code { font-family: var(--font-mono); font-size: 0.86em; overflow-wrap: anywhere; }
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
";

/// hangar-render's stylesheet followed by the server's rules.
pub(crate) fn stylesheet() -> String {
    format!("{}{SERVER_CSS}", hangar_render::stylesheet())
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
    /// hangar itself: the index and the server's messages.
    Hangar,
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
            Self::Hangar => String::new(),
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
            "<div class=\"band\"><a class=\"mark-link\" href=\"/hangar/\">{MARK}</a>{place}</div>\n"
        )
    }
}

/// A hangar document, built part by part.
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
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{refresh}<title>{} - hangar</title>\n<link rel=\"stylesheet\" href=\"/hangar/style.css\">\n</head>\n<body class=\"hangar\">\n{}<main{code}>\n{}\n</main>\n</body>\n</html>\n",
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
    Doc::new("Signed in", Band::Hangar)
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

/// The index at `/hangar/`: each page and its bindings as links.
pub(crate) fn index(entries: &[Entry]) -> String {
    let mut main = String::from("<div class=\"own\"><h1>Pages</h1>");
    if entries.is_empty() {
        main.push_str("<p>No page is published yet. Run <code>hangar publish</code>.</p>");
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
    Doc::new("Pages", Band::Hangar).main(main).finish()
}
