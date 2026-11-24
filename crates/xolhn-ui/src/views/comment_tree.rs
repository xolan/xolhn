use std::ops::Range;

use super::IdCallback;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, FontStyle, FontWeight, HighlightStyle, InteractiveText, Role,
    StyledText, UnderlineStyle, div, px, relative, rems,
};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use xolhn_core::html::{ContentBlock, TextSpan, sanitize_url};
use xolhn_core::settings::AppSettings;

use crate::state::VisibleCommentRow;
use crate::theme_bridge::UiColors;

struct SpanHighlights {
    full_text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    link_ranges: Vec<Range<usize>>,
    link_urls: Vec<String>,
}

fn build_span_highlights(spans: &[TextSpan], colors: &UiColors) -> SpanHighlights {
    let mut full_text = String::new();
    let mut highlights = Vec::new();
    let mut link_ranges = Vec::new();
    let mut link_urls = Vec::new();

    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let start = full_text.len();
        full_text.push_str(&span.text);
        let end = full_text.len();

        if start < end && full_text.is_char_boundary(start) && full_text.is_char_boundary(end) {
            let mut style = HighlightStyle::default();
            if span.bold {
                style.font_weight = Some(FontWeight::BOLD);
            }
            if span.italic {
                style.font_style = Some(FontStyle::Italic);
            }
            if span.code {
                style.background_color = Some(colors.code_bg);
            }
            if let Some(url) = &span.link {
                style.color = Some(colors.link);
                style.underline = Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(colors.link),
                    wavy: false,
                });
                link_ranges.push(start..end);
                link_urls.push(url.clone());
            }

            if style != HighlightStyle::default() {
                highlights.push((start..end, style));
            }
        }
    }

    SpanHighlights {
        full_text,
        highlights,
        link_ranges,
        link_urls,
    }
}

/// Renders parsed Hacker News HTML [`ContentBlock`] items into a styled GPUI element tree.
pub fn render_html_blocks(
    id_prefix: impl Into<String>,
    blocks: &[ContentBlock],
    colors: &UiColors,
    settings: &AppSettings,
    cx: &App,
) -> AnyElement {
    let id_prefix = id_prefix.into();
    let lh = relative(settings.line_height_density.multiplier());
    let mono_font = settings
        .mono_font_family
        .as_deref()
        .unwrap_or_else(|| cx.theme().mono_font_family.as_ref())
        .to_string();

    let mut container = v_flex()
        .w_full()
        .gap_2()
        .text_color(colors.text)
        .text_size(rems(0.92))
        .line_height(lh);

    if let Some(ui_font) = &settings.ui_font_family {
        container = container.font_family(ui_font.clone());
    }

    for (idx, block) in blocks.iter().enumerate() {
        match block {
            ContentBlock::Paragraph(spans) | ContentBlock::Quote(spans) => {
                let is_quote = matches!(block, ContentBlock::Quote(_));
                let SpanHighlights {
                    full_text,
                    highlights,
                    link_ranges,
                    link_urls,
                } = build_span_highlights(spans, colors);

                if full_text.is_empty() {
                    continue;
                }

                let styled = StyledText::new(full_text).with_highlights(highlights);
                let mut interactive = InteractiveText::new(
                    ElementId::Name(format!("{id_prefix}-blk-{idx}").into()),
                    styled,
                );

                if !link_ranges.is_empty() {
                    interactive =
                        interactive.on_click(link_ranges, move |range_ix, _window, cx| {
                            if let Some(url) = link_urls.get(range_ix).and_then(|u| sanitize_url(u))
                            {
                                cx.open_url(&url);
                            }
                        });
                }

                if is_quote {
                    container = container.child(
                        div()
                            .w_full()
                            .border_l_2()
                            .border_color(colors.subtle)
                            .pl_2p5()
                            .py_0p5()
                            .text_color(colors.muted)
                            .child(interactive),
                    );
                } else {
                    container = container.child(div().w_full().child(interactive));
                }
            }
            ContentBlock::CodeBlock(code_text) => {
                container = container.child(
                    div()
                        .w_full()
                        .bg(colors.code_bg)
                        .border_1()
                        .border_color(colors.border)
                        .rounded(px(6.))
                        .p_2()
                        .overflow_x_hidden()
                        .text_size(rems(0.84))
                        .child(
                            div()
                                .font_family(mono_font.clone())
                                .child(code_text.clone()),
                        ),
                );
            }
        }
    }

    container.into_any_element()
}

/// Renders a single flattened comment row with depth rail, collapsible header, OP badge, and body.
pub fn render_comment_row(
    row: &VisibleCommentRow,
    colors: &UiColors,
    settings: &AppSettings,
    now_unix: i64,
    is_mobile: bool,
    on_toggle: IdCallback,
    cx: &App,
) -> AnyElement {
    let comment_id = row.id;
    let rail_color = colors.rails[(row.depth.saturating_sub(1)) % 4];
    let indent_px =
        px(((row.depth.saturating_sub(1)).min(7) as f32) * if is_mobile { 10.0 } else { 14.0 });
    let mono_font = settings
        .mono_font_family
        .as_deref()
        .unwrap_or_else(|| cx.theme().mono_font_family.as_ref())
        .to_string();

    let author_label = row
        .author
        .as_deref()
        .filter(|a| !a.is_empty())
        .unwrap_or("[deleted]");
    let toggle_action = if row.is_collapsed {
        format!("Expand comment by {author_label}")
    } else {
        format!("Collapse comment by {author_label}")
    };

    let mut header = div()
        .id(("comment-head", row.id))
        .role(Role::Button)
        .aria_label(toggle_action)
        .aria_expanded(!row.is_collapsed)
        .w_full()
        .flex()
        .items_center()
        .gap_1p5()
        .cursor_pointer()
        .py_0p5()
        .text_size(rems(0.82));

    if is_mobile {
        header = header.min_h(px(32.));
    }

    let author_el = match row.author.as_deref().filter(|a| !a.is_empty()) {
        Some(author) => div()
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .child(author.to_string()),
        None => div()
            .font_weight(FontWeight::BOLD)
            .text_color(colors.subtle)
            .child("[deleted]"),
    };
    header = header.child(author_el);

    if row.is_op {
        header = header.child(
            div()
                .px_1p5()
                .py_0p5()
                .rounded(px(3.))
                .bg(colors.accent_soft)
                .text_color(colors.accent)
                .font_weight(FontWeight::BOLD)
                .text_size(rems(0.72))
                .child("OP"),
        );
    }

    let time = row.relative_time(now_unix);
    if !time.is_empty() {
        header = header.child(div().text_color(colors.muted).child(format!("· {time}")));
    }

    let collapse_indicator = if row.is_collapsed {
        format!("[+{}]", row.subtree_count)
    } else {
        "[-]".to_string()
    };
    header = header.child(
        div()
            .ml_auto()
            .font_family(mono_font)
            .text_color(colors.subtle)
            .child(collapse_indicator),
    );

    header = header.on_click(move |_, window, cx| {
        on_toggle(comment_id, window, cx);
    });

    let mut card = v_flex()
        .w_full()
        .pl_2p5()
        .border_l_2()
        .border_color(rail_color)
        .gap_1()
        .child(header);

    if !row.is_collapsed && !row.blocks.is_empty() {
        card = card.child(render_html_blocks(
            format!("comment-{}", row.id),
            &row.blocks,
            colors,
            settings,
            cx,
        ));
    }

    div()
        .id(("comment-row", row.id))
        .w_full()
        .pl(indent_px)
        .mt_1p5()
        .child(card)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use xolhn_core::theme::ThemeId;

    #[test]
    fn span_highlights_track_utf8_ranges_and_links() {
        let colors = UiColors::from(ThemeId::XolhnDark);
        let spans = vec![
            TextSpan {
                text: "Hello 🌍 ".to_string(),
                italic: false,
                bold: false,
                code: false,
                link: None,
            },
            TextSpan {
                text: "bold-link".to_string(),
                italic: false,
                bold: true,
                code: false,
                link: Some("https://news.ycombinator.com".to_string()),
            },
            TextSpan {
                text: " and ".to_string(),
                italic: false,
                bold: false,
                code: false,
                link: None,
            },
            TextSpan {
                text: "code()".to_string(),
                italic: true,
                bold: false,
                code: true,
                link: None,
            },
        ];

        let built = build_span_highlights(&spans, &colors);
        assert_eq!(built.full_text, "Hello 🌍 bold-link and code()");
        assert_eq!(built.highlights.len(), 2);
        assert_eq!(built.link_ranges.len(), 1);
        assert_eq!(built.link_urls, vec!["https://news.ycombinator.com"]);

        let link_range = &built.link_ranges[0];
        assert_eq!(&built.full_text[link_range.clone()], "bold-link");
        assert_eq!(built.highlights[0].0, link_range.clone());
        assert_eq!(built.highlights[0].1.font_weight, Some(FontWeight::BOLD));
        assert_eq!(built.highlights[0].1.color, Some(colors.link));
        assert!(built.highlights[0].1.underline.is_some());

        let code_range = &built.highlights[1].0;
        assert_eq!(&built.full_text[code_range.clone()], "code()");
        assert_eq!(built.highlights[1].1.font_style, Some(FontStyle::Italic));
        assert_eq!(built.highlights[1].1.background_color, Some(colors.code_bg));
    }
}
