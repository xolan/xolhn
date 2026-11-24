use super::{ActionCallback, IdCallback, UrlCallback};

use gpui::{
    AnyElement, FontWeight, InteractiveElement as _, IntoElement as _, MouseButton,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, div,
    prelude::FluentBuilder as _, px, relative, rems, transparent_black,
};
use gpui_kit::component::{
    Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use xolhn_core::model::Story;

use crate::theme_bridge::UiColors;

/// Renders a desktop story row for the left/middle feed pane.
#[allow(clippy::too_many_arguments)]
pub fn render_desktop_story_row(
    rank: usize,
    story: &Story,
    selected: bool,
    colors: &UiColors,
    now_unix: i64,
    mono_font: &str,
    on_select: IdCallback,
    on_open_url: UrlCallback,
) -> AnyElement {
    let story_id = story.id;
    let domain = story.display_domain();
    let target_url = story.effective_url();
    let has_external_url = story
        .url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());
    let rel_time = story.relative_time(now_unix);
    let meta_text = format!("{domain} · {}p · {rel_time}", story.points);
    let comment_count = story.num_comments.to_string();
    let mono_family = mono_font.to_string();

    let row_bg = if selected {
        colors.surface_2
    } else {
        transparent_black()
    };
    let left_border_color = if selected {
        colors.accent
    } else {
        transparent_black()
    };
    let hover_bg = colors.hover;

    div()
        .id(("desktop-story-row", story_id))
        .role(Role::Button)
        .aria_label(format!(
            "{rank}. {} ({domain}, {} comments)",
            story.title, story.num_comments
        ))
        .aria_selected(selected)
        .w_full()
        .border_b_1()
        .border_color(colors.border)
        .bg(row_bg)
        .hover(move |s| s.bg(hover_bg))
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            on_select(story_id, window, cx);
        })
        .child(
            h_flex()
                .w_full()
                .px_2p5()
                .py_2()
                .gap_2()
                .items_start()
                .border_l_3()
                .border_color(left_border_color)
                .child(
                    div()
                        .w(px(18.))
                        .flex_shrink_0()
                        .pt(px(1.))
                        .font_family(mono_family.clone())
                        .text_size(rems(0.74))
                        .text_color(if selected {
                            colors.accent
                        } else {
                            colors.subtle
                        })
                        .font_weight(if selected {
                            FontWeight::BOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .child(rank.to_string()),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .line_clamp(2)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(rems(0.86))
                                .line_height(relative(1.3))
                                .text_color(colors.text)
                                .child(story.title.clone()),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap_1p5()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(rems(0.74))
                                        .text_color(colors.subtle)
                                        .child(meta_text),
                                )
                                .child(
                                    h_flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .px_1p5()
                                                .py(px(1.))
                                                .rounded_full()
                                                .bg(colors.accent_soft)
                                                .text_color(colors.accent)
                                                .font_family(mono_family)
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_size(rems(0.70))
                                                .child(comment_count),
                                        )
                                        .when(has_external_url, |actions| {
                                            actions.child(
                                                div()
                                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                        cx.stop_propagation();
                                                    })
                                                    .child(
                                                        Button::new((
                                                            "desktop-story-url",
                                                            story_id,
                                                        ))
                                                        .ghost()
                                                        .xsmall()
                                                        .icon(IconName::ExternalLink)
                                                        .accessibility_label(format!(
                                                            "Open {domain} in browser"
                                                        ))
                                                        .tooltip("Open link in browser")
                                                        .on_click(move |_, window, cx| {
                                                            cx.stop_propagation();
                                                            on_open_url(
                                                                target_url.clone(),
                                                                window,
                                                                cx,
                                                            );
                                                        }),
                                                    ),
                                            )
                                        }),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

/// Renders a mobile story card where tapping the story opens comments by default and
/// a secondary button opens the external article URL.
pub fn render_mobile_story_card(
    story: &Story,
    selected: bool,
    colors: &UiColors,
    now_unix: i64,
    mono_font: &str,
    on_open_url: UrlCallback,
    on_open_thread: IdCallback,
) -> AnyElement {
    let story_id = story.id;
    let target_url = story.effective_url();
    let has_external_url = story
        .url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());
    let domain = story.display_domain();
    let rel_time = story.relative_time(now_unix);
    let points_and_time = format!("· {} pts · {rel_time}", story.points);
    let comment_count = story.num_comments.to_string();
    let mono_family = mono_font.to_string();

    let card_bg = if selected {
        colors.surface_2
    } else {
        transparent_black()
    };
    let hover_bg = colors.hover;

    h_flex()
        .id(("mobile-story-card", story_id))
        .w_full()
        .items_center()
        .gap_2p5()
        .px_3()
        .py_2p5()
        .border_b_1()
        .border_color(colors.border)
        .bg(card_bg)
        .child(
            v_flex()
                .id(("mobile-story-thread", story_id))
                .role(Role::Button)
                .aria_label(format!(
                    "Open {} comments for {}",
                    story.num_comments, story.title
                ))
                .aria_selected(selected)
                .flex_1()
                .min_w_0()
                .gap_1()
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    on_open_thread(story_id, window, cx);
                })
                .child(
                    div()
                        .text_size(rems(0.92))
                        .font_weight(FontWeight::SEMIBOLD)
                        .line_height(relative(1.32))
                        .text_color(colors.text)
                        .child(story.title.clone()),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1p5()
                        .flex_wrap()
                        .text_size(rems(0.75))
                        .text_color(colors.muted)
                        .child(
                            div()
                                .px_1p5()
                                .py(px(1.))
                                .rounded(px(6.))
                                .bg(colors.accent_soft)
                                .text_color(colors.accent)
                                .font_family(mono_family)
                                .font_weight(FontWeight::BOLD)
                                .text_size(rems(0.72))
                                .child(format!("{comment_count} cmt")),
                        )
                        .child(
                            div()
                                .text_color(colors.subtle)
                                .font_weight(FontWeight::MEDIUM)
                                .child(domain.clone()),
                        )
                        .child(div().text_color(colors.muted).child(points_and_time)),
                ),
        )
        .when(has_external_url, |card| {
            card.child(
                div()
                    .id(("mobile-story-url", story_id))
                    .role(Role::Link)
                    .aria_label(format!("Open {domain} in browser"))
                    .flex_shrink_0()
                    .w(px(44.))
                    .h(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.surface)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg).border_color(colors.accent))
                    .on_click(move |_, window, cx| {
                        on_open_url(target_url.clone(), window, cx);
                    })
                    .child(
                        Icon::new(IconName::ExternalLink)
                            .small()
                            .text_color(colors.link),
                    ),
            )
        })
        .into_any_element()
}

/// Renders the footer at the bottom of a story feed list (loading indicator, load-more button, or end-of-feed label).
pub fn render_feed_footer(
    has_more: bool,
    loading_more: bool,
    colors: &UiColors,
    on_load_more: ActionCallback,
) -> AnyElement {
    if loading_more {
        h_flex()
            .w_full()
            .items_center()
            .justify_center()
            .gap_2()
            .px_3()
            .py_3()
            .text_size(rems(0.80))
            .text_color(colors.muted)
            .child(
                Icon::new(IconName::LoaderCircle)
                    .small()
                    .text_color(colors.accent),
            )
            .child("Loading more...")
            .into_any_element()
    } else if has_more {
        div()
            .w_full()
            .px_3()
            .py_2p5()
            .child(
                Button::new("feed-load-more")
                    .secondary()
                    .small()
                    .w_full()
                    .label("Load more stories")
                    .on_click(move |_, window, cx| {
                        on_load_more(window, cx);
                    }),
            )
            .into_any_element()
    } else {
        h_flex()
            .w_full()
            .items_center()
            .justify_center()
            .px_3()
            .py_3()
            .text_size(rems(0.76))
            .text_color(colors.subtle)
            .child("End of feed")
            .into_any_element()
    }
}

/// Renders a centered loading placeholder for a story or search list.
pub fn render_list_loading(label: &str, colors: &UiColors) -> AnyElement {
    v_flex()
        .w_full()
        .items_center()
        .justify_center()
        .gap_2()
        .px_4()
        .py_8()
        .child(
            Icon::new(IconName::LoaderCircle)
                .large()
                .text_color(colors.accent),
        )
        .child(
            div()
                .text_size(rems(0.82))
                .text_color(colors.muted)
                .text_center()
                .child(label.to_string()),
        )
        .into_any_element()
}

/// Renders a centered empty-state message with title and subtitle.
pub fn render_list_empty(title: &str, subtitle: &str, colors: &UiColors) -> AnyElement {
    let mut container = v_flex()
        .w_full()
        .items_center()
        .justify_center()
        .gap_1p5()
        .px_4()
        .py_8()
        .child(Icon::new(IconName::Inbox).large().text_color(colors.subtle))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(rems(0.90))
                .text_color(colors.text)
                .text_center()
                .child(title.to_string()),
        );

    if !subtitle.is_empty() {
        container = container.child(
            div()
                .text_size(rems(0.78))
                .text_color(colors.muted)
                .text_center()
                .child(subtitle.to_string()),
        );
    }

    container.into_any_element()
}

/// Renders a centered error state with an alert icon, message, and retry button.
pub fn render_list_error(message: &str, colors: &UiColors, on_retry: ActionCallback) -> AnyElement {
    v_flex()
        .w_full()
        .items_center()
        .justify_center()
        .gap_2p5()
        .px_4()
        .py_8()
        .child(
            Icon::new(IconName::CircleAlert)
                .large()
                .text_color(colors.accent),
        )
        .child(
            div()
                .text_size(rems(0.82))
                .text_color(colors.text)
                .text_center()
                .child(message.to_string()),
        )
        .child(
            Button::new("story-list-retry")
                .secondary()
                .small()
                .icon(IconName::RefreshCw)
                .label("Retry")
                .on_click(move |_, window, cx| {
                    on_retry(window, cx);
                }),
        )
        .into_any_element()
}
