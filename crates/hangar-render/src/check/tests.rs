//! Unit tests of the template check (threats 9, 10, 13, 14 and 21; plan
//! tests 2 to 5).

use super::*;
use crate::test_support::{Tok, html_page, markdown_page, tokens};
use proptest::prelude::*;

fn flaws(template: &str) -> Vec<FlawKind> {
    match check_page(&html_page(template, Vec::new())) {
        Ok(()) => Vec::new(),
        Err(flaws) => flaws.iter().map(|flaw| flaw.kind().clone()).collect(),
    }
}

#[track_caller]
fn admitted(template: &str) {
    assert_eq!(flaws(template), Vec::<FlawKind>::new(), "{template}");
}

#[track_caller]
fn refused(template: &str, kind: FlawKind) {
    assert!(
        flaws(template).contains(&kind),
        "{template}: {:?}",
        flaws(template)
    );
}

#[test]
fn a_mustache_tag_in_a_url_or_style_attribute_is_refused_and_a_fixed_value_is_admitted() {
    for attribute in [
        "href",
        "src",
        "action",
        "formaction",
        "srcset",
        "ping",
        "poster",
        "data",
        "cite",
        "background",
        "xlink:href",
        "style",
    ] {
        refused(
            &format!("<div {attribute}=\"{{{{v}}}}\">x</div>"),
            FlawKind::TagInUrlOrStyle {
                attribute: attribute.to_owned(),
            },
        );
        refused(
            &format!("<div {}=\"/a/{{{{v}}}}\">x</div>", attribute.to_uppercase()),
            FlawKind::TagInUrlOrStyle {
                attribute: attribute.to_owned(),
            },
        );
        admitted(&format!("<div {attribute}=\"/fixed\">x</div>"));
    }
}

#[test]
fn a_mustache_tag_in_an_svg_animation_value_is_refused_because_it_can_set_a_link() {
    for attribute in ["to", "from", "values", "by", "attributename"] {
        refused(
            &format!(
                "<svg><a href=\"/x\"><set attributeName=\"href\" {attribute}=\"{{{{v}}}}\"/></a></svg>"
            ),
            FlawKind::TagInUrlOrStyle {
                attribute: attribute.to_owned(),
            },
        );
    }
    admitted("<svg><rect width=\"1\"><animate attributeName=\"width\" to=\"10\"/></rect></svg>");
}

#[test]
fn a_mustache_tag_in_an_svg_attribute_that_takes_url_or_in_attributionsrc_is_refused() {
    for attribute in [
        "fill",
        "stroke",
        "filter",
        "clip-path",
        "mask",
        "marker-start",
        "marker-mid",
        "marker-end",
        "cursor",
    ] {
        refused(
            &format!("<svg><rect {attribute}=\"{{{{v}}}}\"/></svg>"),
            FlawKind::TagInUrlOrStyle {
                attribute: attribute.to_owned(),
            },
        );
        admitted(&format!("<svg><rect {attribute}=\"none\"/></svg>"));
    }
    refused(
        "<img attributionsrc=\"{{v}}\" alt=\"\">",
        FlawKind::TagInUrlOrStyle {
            attribute: "attributionsrc".to_owned(),
        },
    );
}

#[test]
fn a_mustache_tag_in_another_double_quoted_attribute_is_admitted() {
    admitted("<p title=\"{{v}}\" class=\"card {{status}}\" data-x=\"{{a.b}}\">{{v}}</p>");
    admitted("<svg viewBox=\"0 0 10 10\"><text x=\"1\" aria-label=\"{{v}}\">{{v}}</text></svg>");
}

#[test]
fn a_mustache_tag_in_a_style_element_or_title_is_refused_and_fixed_content_is_admitted() {
    refused("<style>p { color: {{c}} }</style>", FlawKind::TagInRawText);
    refused("<title>{{v}}</title>", FlawKind::TagInRawText);
    admitted("<style>p { color: red }</style><title>Fixed</title>");
}

#[test]
fn a_mustache_tag_in_an_unquoted_value_is_refused_and_its_double_quoted_twin_is_admitted() {
    refused("<p title={{v}}>x</p>", FlawKind::TagInUnquotedValue);
    refused("<p title=a{{v}}>x</p>", FlawKind::TagInUnquotedValue);
    admitted("<p title=\"{{v}}\">x</p>");
}

#[test]
fn a_mustache_tag_in_a_single_quoted_value_is_refused_and_its_double_quoted_twin_is_admitted() {
    refused("<p title='{{v}}'>x</p>", FlawKind::TagInSingleQuotedValue);
    admitted("<p title=\"{{v}}\">x</p>");
}

#[test]
fn a_mustache_tag_in_a_comment_or_declaration_is_refused_and_one_beside_it_is_admitted() {
    refused("<!-- {{v}} -->", FlawKind::TagInComment);
    refused("<!DOCTYPE {{v}}>", FlawKind::TagInComment);
    refused("<!x {{v}}>", FlawKind::TagInComment);
    admitted("<!-- a note -->{{v}}");
}

#[test]
fn a_mustache_tag_in_tag_position_is_refused_and_one_in_text_is_admitted() {
    refused("<p {{v}}>x</p>", FlawKind::TagInMarkup);
    refused("<p{{v}}>x</p>", FlawKind::TagInMarkup);
    refused("<p title=\"x\"{{v}}>x</p>", FlawKind::TagInMarkup);
    refused("<p>x</p title=\"{{v}}\">", FlawKind::TagInMarkup);
    refused("<{{v}}>", FlawKind::TagAfterLessThan);
    refused("</{{v}}>", FlawKind::TagAfterLessThan);
    refused("<{{#s}}{{/s}}script>", FlawKind::TagAfterLessThan);
    admitted("<p>{{v}}</p> a < {{v}} and a &{{v}}");
}

#[test]
fn a_section_inside_an_html_tag_is_refused_and_one_around_it_is_admitted() {
    refused("<p {{#s}}hidden{{/s}}>x</p>", FlawKind::SectionInMarkup);
    refused(
        "<p title=\"{{#s}}a{{/s}}\">x</p>",
        FlawKind::SectionInMarkup,
    );
    admitted("{{#s}}<p title=\"a\">x</p>{{/s}}{{^s}}<p>none</p>{{/s}}");
    admitted("<p title=\"{{! a note }}\">x</p>");
}

#[test]
fn each_forbidden_element_is_refused_in_any_case() {
    for element in [
        "script",
        "base",
        "link",
        "iframe",
        "frame",
        "object",
        "embed",
        "form",
        "input",
        "button",
        "select",
        "textarea",
        "xmp",
        "plaintext",
        "noscript",
        "noembed",
        "noframes",
    ] {
        let kind = FlawKind::ForbiddenElement {
            element: element.to_owned(),
        };
        refused(&format!("<{element}>"), kind.clone());
        refused(
            &format!("<{} id=\"x\">", element.to_uppercase()),
            kind.clone(),
        );
        refused(&format!("<p>a</p><{element}\nclass=\"y\" />"), kind);
    }
}

#[test]
fn meta_with_http_equiv_is_refused_in_any_case_and_order_and_meta_charset_is_admitted() {
    let kind = FlawKind::ForbiddenElement {
        element: "meta".to_owned(),
    };
    refused(
        "<meta http-equiv=\"refresh\" content=\"0;url=/x\">",
        kind.clone(),
    );
    refused(
        "<META CONTENT=\"0;url=/x\" HTTP-EQUIV=\"refresh\">",
        kind.clone(),
    );
    refused("<meta content='0' http-equiv=refresh>", kind);
    admitted("<meta charset=\"utf-8\">");
}

#[test]
fn an_event_handler_attribute_is_refused_in_any_case_and_quoting() {
    for template in [
        "<p onclick=\"x()\">a</p>",
        "<P ONMOUSEOVER='x()'>a</P>",
        "<svg onload=x()></svg>",
        "<img alt=\"a\" onerror>",
    ] {
        assert!(
            flaws(template)
                .iter()
                .any(|kind| matches!(kind, FlawKind::EventHandler { .. })),
            "{template}"
        );
    }
    admitted("<p data-on=\"x\" title=\"on\">a</p>");
}

#[test]
fn inline_svg_is_admitted() {
    admitted(
        "<svg viewBox=\"0 0 100 20\" role=\"img\"><rect x=\"0\" width=\"{{done}}\" height=\"20\"/><text x=\"2\" y=\"14\">{{label}}</text></svg>",
    );
}

#[test]
fn constructs_that_parse_differently_by_context_are_refused() {
    refused("<style>a<b</style>", FlawKind::LessThanInRawText);
    refused("<title>a</b></title>", FlawKind::LessThanInRawText);
    refused("<svg><![CDATA[x]]></svg>", FlawKind::CData);
    refused("<?xml version=\"1.0\"?>", FlawKind::ProcessingInstruction);
    refused("<!-->", FlawKind::MalformedComment);
    refused("<!--->", FlawKind::MalformedComment);
    refused("<!-- a <!-- b -->", FlawKind::MalformedComment);
    refused("<!-- a --!>", FlawKind::MalformedComment);
    refused("a\0b", FlawKind::Nul);
    admitted("<style>a{}</style><title>a &amp; b</title><!-- a - b -- c --><!---->");
}

#[test]
fn a_template_that_ends_inside_a_tag_or_comment_is_refused() {
    refused("<p title=\"x", FlawKind::Unterminated);
    refused("<p", FlawKind::Unterminated);
    refused("<!-- open", FlawKind::Unterminated);
    admitted("a < b");
}

#[test]
fn an_attribute_given_twice_is_refused() {
    refused(
        "<p title=\"a\" TITLE=\"{{v}}\">x</p>",
        FlawKind::DuplicateAttribute {
            attribute: "title".to_owned(),
        },
    );
    admitted("<p title=\"a\" class=\"{{v}}\">x</p>");
}

#[test]
fn triple_ampersand_partial_and_set_delimiter_tags_are_refused() {
    for template in [
        "{{{v}}}",
        "{{&v}}",
        "{{> p}}",
        "{{>p}}",
        "{{=<% %>=}}<%v%>",
        "{{<parent}}{{/parent}}",
        "{{$block}}{{/block}}",
    ] {
        refused(template, FlawKind::ForbiddenMustacheTag);
    }
    admitted("{{v}}{{#s}}{{/s}}{{^s}}{{/s}}{{! note }}{{ spaced }}");
}

#[test]
fn malformed_mustache_tags_are_refused() {
    for template in [
        "{{v",
        "{{a b}}",
        "{{é}}",
        "{{! é }}",
        "{{a{{b}}}}",
        "{{}}",
        "{{#}}{{/}}",
    ] {
        refused(template, FlawKind::MalformedMustacheTag);
    }
    admitted("{{a.b-c_d}}{{.}}");
}

#[test]
fn nine_nested_sections_are_refused_and_eight_are_admitted() {
    let nested = |depth: usize| {
        let open: String = (0..depth)
            .map(|level| format!("{{{{#s{level}}}}}"))
            .collect();
        let close: String = (0..depth)
            .rev()
            .map(|level| format!("{{{{/s{level}}}}}"))
            .collect();
        format!("{open}x{close}")
    };
    admitted(&nested(8));
    refused(&nested(9), FlawKind::TooDeep);
}

#[test]
fn unbalanced_sections_are_refused() {
    let unbalanced = |name: &str| FlawKind::UnbalancedSection {
        name: name.to_owned(),
    };
    refused("{{#a}}x", unbalanced("a"));
    refused("x{{/a}}", unbalanced("a"));
    refused("{{#a}}{{#b}}{{/a}}{{/b}}", unbalanced("a"));
    admitted("{{#a}}{{#b}}x{{/b}}{{/a}}");
}

#[test]
fn a_flaw_names_its_line_and_its_column_in_characters() {
    let flaws = check_page(&html_page("<p>é</p>\n  é<p title='{{v}}'>", Vec::new())).unwrap_err();
    let flaw = flaws.iter().next().unwrap();
    assert_eq!((flaw.line(), flaw.column()), (2, 14));
    assert_eq!(flaws.len(), 1);
}

#[test]
fn every_flaw_is_reported_in_template_order() {
    let flaws = check_page(&html_page(
        "<script></script>\n<p onclick=\"x\">{{{v}}}</p>",
        Vec::new(),
    ))
    .unwrap_err();
    let lines: Vec<usize> = flaws.iter().map(Flaw::line).collect();
    assert_eq!(lines, vec![1, 2, 2]);
}

#[test]
fn a_markdown_page_always_passes_the_check() {
    assert_eq!(
        check_page(&markdown_page("<script>{{{x}}}</script>")),
        Ok(())
    );
}

/// Pieces that templates are built from, so that many generated templates
/// pass the check.
fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => prop::sample::select(vec![
            "text ", "&amp; ", "a > b ", "\"q\" 'q' ", "\n", "  ", "<p>", "</p>", "<br/>",
            "<svg viewBox=\"0 0 1 1\"><circle r=\"1\"/></svg>", "<x-y a=b c>", "<!-- c -->",
            "<!DOCTYPE html>", "<style>p { color: red }</style>", "<title>t</title>", "{{v}}",
            "{{a.b}}", "{{.}}", "{{#s}}", "{{/s}}", "{{^s}}", "{{! note }}", "<a title=\"", "\">",
            "<a class='", "'>", "<img alt=", " >", "<", "</", " x=\"1\"", "<b\nid=\"", "\"/>",
            "<meta charset=\"utf-8\">", "&", "<pre>", "</pre>",
        ])
        .prop_map(str::to_owned),
        1 => "[a-z<>\"'=/ !{}&#-]{0,6}",
    ]
}

fn template() -> impl Strategy<Value = String> {
    prop::collection::vec(piece(), 0..24).prop_map(|pieces| pieces.concat())
}

/// Where a browser's tokenizer puts each marker in `html`.
fn placed(html: &str, count: usize) -> Vec<Place> {
    let tokens = tokens(html);
    (0..count)
        .map(|index| {
            let marker = format!("zq{index}zq");
            for token in &tokens {
                match token {
                    Tok::Text(text) if text.contains(&marker) => return Place::Text,
                    Tok::Start { name, attributes } => {
                        if let Some((attribute, _)) =
                            attributes.iter().find(|(_, value)| value.contains(&marker))
                        {
                            return Place::DoubleQuoted {
                                element: name.clone(),
                                attribute: attribute.clone(),
                                end_tag: false,
                            };
                        }
                    }
                    _ => {}
                }
            }
            Place::Markup
        })
        .collect()
}

proptest! {
    // About two in three generated templates fail the check and are
    // rejected, so the run allows many more rejects than proptest's default.
    #![proptest_config(ProptestConfig {
        max_global_rejects: 1 << 16,
        ..ProptestConfig::default()
    })]

    #[test]
    fn a_browser_places_each_variable_of_an_admitted_template_where_the_check_said(template in template()) {
        let page = html_page(&template, Vec::new());
        prop_assume!(check_page(&page).is_ok());
        let scanned = scan(&template);
        let mut marked = String::new();
        let mut expected = Vec::new();
        let mut rest = 0;
        for tag in mustache::lex(&template).0 {
            marked.push_str(&template[rest..tag.start]);
            rest = tag.end;
            if tag.kind == TagKind::Variable {
                marked.push_str(&format!("zq{}zq", expected.len()));
                let site = scanned.sites.iter().find(|site| site.at == tag.start).unwrap();
                expected.push(site.place.clone());
            }
        }
        marked.push_str(&template[rest..]);
        prop_assert_eq!(placed(&marked, expected.len()), expected, "{}", marked);
    }

    #[test]
    fn the_check_never_panics(template in template(), junk in "\\PC{0,40}") {
        let _ = check_page(&html_page(&format!("{template}{junk}"), Vec::new()));
    }
}
