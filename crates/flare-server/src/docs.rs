//! flare's own documents: the shell with the band, the index, the launch
//! document and the stylesheet. Page content never enters these documents
//! except as the escaped `srcdoc` of a frame (`frame.rs`, threat 12).

/// The band's mark: an arched outline with a door, beside the name.
const MARK: &str = "<span class=\"mark\"><svg viewBox=\"0 0 20 20\" aria-hidden=\"true\"><path d=\"M2 17V9q8-7 16 0v8Z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\"/><path d=\"M7 17v-5h6v5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\"/></svg>flare</span>";

/// The rules for the server's own markup. flare-render's stylesheet comes
/// first and holds the tokens these rules use.
const SERVER_CSS: &str = r#"
/* The fonts come from this origin, at the names `assets::FONTS` serves. */
@font-face { font-family: "IBM Plex Sans"; font-style: normal; font-weight: 100 700; font-display: swap; src: url("/flare/assets/ibm-plex-sans-latin-e2291e84.woff2") format("woff2"); unicode-range: U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD; }
@font-face { font-family: "JetBrains Mono"; font-style: normal; font-weight: 100 800; font-display: swap; src: url("/flare/assets/jetbrains-mono-latin-18be4527.woff2") format("woff2"); unicode-range: U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD; }
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
/* The foot below the article and the Get Flare link (the badge mockup, option a) */
.share-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px 16px; flex-wrap: wrap; padding: 14px 32px; border-top: 1px solid var(--line); }
.share-foot .note { font-size: 12px; color: var(--muted); }
.share-foot .badge { display: inline-flex; align-items: center; gap: 8px; min-height: 32px; padding: 0 12px; border: 1px solid var(--line-strong); border-radius: var(--radius); background: var(--card); color: var(--fg); font: 500 13px var(--font-body); text-decoration: none; white-space: nowrap; }
.share-foot .badge svg { width: 14px; height: 14px; fill: var(--accent); flex: none; }
.share-foot .badge .arr { color: var(--muted); }
.share-foot .badge:hover { border-color: var(--accent); background: var(--accent-tint); color: var(--accent-ink); }
.share-foot .badge:hover .arr { color: var(--accent-ink); }
.share-foot .badge:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
/* The comment count under the title (Flare mockup v2, artboard 01) */
.share-meta .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
body.reading a.chip { display: inline-flex; align-items: center; gap: 6px; border: 1px solid var(--line); border-radius: var(--radius); padding: 2px 8px; font-size: 12px; background: var(--card); white-space: nowrap; color: var(--fg); text-decoration: none; }
body.reading a.chip:hover { text-decoration: underline; }
body.reading a.chip:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
/* The comments section (comment parts mockup, artboards 01 to 07) */
.comments .thread { max-width: 820px; width: 100%; margin-inline: auto; display: flex; flex-direction: column; gap: 12px; min-width: 0; }
.comments h2 { margin: 0; font-size: 20px; line-height: 1.25; font-weight: 600; text-wrap: balance; }
.comments p { margin: 0; }
.comments .section-head { display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; }
.comments .muted { color: var(--muted); }
.comments .status { font-size: 13px; color: var(--muted); }
.comments .late { visibility: hidden; font-size: 13px; color: var(--muted); animation: comments-late 0s 10s forwards; }
@keyframes comments-late { to { visibility: visible; } }
.comments .card { background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
.comments .skeleton { height: 12px; border-radius: var(--radius); background: var(--line-soft); }
.comments .c-head { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; font-size: 13px; }
.comments .c-head b { font-weight: 600; }
.comments .c-head time { color: var(--muted); font-size: 12px; }
.comments .c-body { font-size: 14px; max-width: 68ch; white-space: pre-wrap; overflow-wrap: anywhere; }
.comments .c-body.deleted { color: var(--muted); font-style: italic; }
.comments .reply { border-top: 1px solid var(--line-soft); padding-top: 10px; padding-left: 36px; display: flex; flex-direction: column; gap: 6px; }
.comments textarea, .comments input[type="text"] { font: 14px/1.45 var(--font-body); color: var(--fg); background: var(--card); border: 1px solid var(--line-strong); border-radius: var(--radius); padding: 8px 10px; width: 100%; resize: vertical; }
.comments input[type="text"] { height: 32px; padding-block: 0; }
.comments textarea[readonly], .comments input[readonly] { background: var(--surface-2); color: var(--muted); }
.comments .field-row { display: flex; gap: 8px; align-items: center; }
.comments .field-row input { flex: 1; min-width: 0; }
.comments .actions { display: flex; gap: 8px; justify-content: flex-end; align-items: center; flex-wrap: wrap; }
.comments .cap { font-size: 12px; color: var(--muted); }
.comments .cap.spread { margin-right: auto; }
.comments .compose { display: flex; flex-direction: column; gap: 8px; border-top: 1px solid var(--line); padding-top: 16px; margin-top: 6px; }
.comments .compose label { font-weight: 500; }
.comments .alert { display: flex; gap: 8px 12px; align-items: center; flex-wrap: wrap; border-radius: var(--radius); padding: 8px 10px; font-size: 13px; }
.comments .alert p { flex: 1 1 220px; }
.comments .alert.amber { background: var(--amber-tint); color: var(--amber-ink); }
.comments .alert.red { background: var(--red-tint); color: var(--red-ink); }
.comments .btn { background: var(--accent); color: var(--on-accent); border: 0; border-radius: var(--radius); padding: 0 14px; height: 32px; font: 500 14px var(--font-body); white-space: nowrap; cursor: pointer; }
.comments .btn[aria-disabled="true"] { opacity: .5; cursor: default; }
.comments .btn[aria-busy="true"] { opacity: .75; cursor: progress; }
.comments .ghost { background: var(--card); border: 1px solid var(--line-strong); border-radius: var(--radius); padding: 0 12px; height: 32px; font: 13px var(--font-body); color: var(--fg); white-space: nowrap; cursor: pointer; }
.comments .ghost[aria-busy="true"] { color: var(--muted); cursor: progress; }
.comments .alert .ghost { height: 28px; }
.comments :is(button, textarea, input):focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
@media (max-width: 640px) {
  .comments .reply { padding-left: 16px; }
  .bar { padding: 10px 16px; }
  .screen { padding: 20px 16px 28px; }
  .share-foot { padding: 14px 16px; flex-direction: column; align-items: stretch; }
  .share-foot .badge { min-height: 44px; justify-content: center; }
}
"#;

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
