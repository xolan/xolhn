use std::sync::Arc;

use super::{ActionCallback, IdCallback, SearchFilterCallback, UrlCallback};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Entity, InteractiveElement as _, IntoElement as _, ListState, MouseButton,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex};
use xolhn_core::model::{
    SearchCommentHit, SearchFilter, SearchHit, SearchResults, SearchSort, Story,
};

use crate::pull_to_refresh::{PullToRefreshState, PullToRefreshTarget, pull_to_refresh};
use crate::smooth_scroll::{SmoothScrollState, SmoothScrollTarget, smooth_scroll};
use crate::state::{LoadState, PreparedSearchHit};
use crate::theme_bridge::UiColors;

/// Returns the number of list items rendered in [`gpui::list`] when search hits are non-empty.
pub fn search_list_item_count(
    hit_count: usize,
    has_inline_error: bool,
    has_more_pages: bool,
) -> usize {
    if hit_count == 0 {
        0
    } else {
        hit_count + usize::from(has_inline_error) + usize::from(has_more_pages)
    }
}

#[derive(Clone)]
pub struct SearchCallbacks {
    pub on_submit: ActionCallback,
    pub on_refresh: ActionCallback,
    pub on_clear: ActionCallback,
    pub on_set_filter: SearchFilterCallback,
    pub on_toggle_sort: ActionCallback,
    pub on_select_story: IdCallback,
    pub on_open_url: UrlCallback,
    pub on_load_more: ActionCallback,
}

#[allow(clippy::too_many_arguments)]
pub fn render_search_panel(
    search_input: &Entity<InputState>,
    filter: SearchFilter,
    sort: SearchSort,
    results: &LoadState<SearchResults>,
    hits: Arc<Vec<PreparedSearchHit>>,
    loading_more: bool,
    refreshing: bool,
    selected_story_id: Option<u64>,
    colors: &UiColors,
    list_state: &ListState,
    smooth_scroll_state: &SmoothScrollState,
    pull_to_refresh_state: &PullToRefreshState,
    smooth_scrolling: bool,
    is_mobile: bool,
    now_unix: i64,
    callbacks: SearchCallbacks,
) -> AnyElement {
    let ui_colors = *colors;
    let has_active_state = !results.is_idle() || !hits.is_empty();

    let header = render_search_header(
        search_input,
        has_active_state,
        &ui_colors,
        is_mobile,
        &callbacks,
    );
    let toolbar = render_filter_sort_toolbar(filter, sort, results, &ui_colors, &callbacks);
    let body = render_results_body(
        results,
        hits,
        loading_more,
        refreshing,
        selected_story_id,
        &ui_colors,
        list_state,
        smooth_scroll_state,
        pull_to_refresh_state,
        smooth_scrolling,
        is_mobile,
        now_unix,
        &callbacks,
    );

    v_flex()
        .id("search-panel")
        .size_full()
        .min_h_0()
        .bg(ui_colors.bg)
        .text_color(ui_colors.text)
        .child(header)
        .child(toolbar)
        .child(body)
        .into_any_element()
}

fn render_search_header(
    search_input: &Entity<InputState>,
    has_active_state: bool,
    colors: &UiColors,
    is_mobile: bool,
    callbacks: &SearchCallbacks,
) -> AnyElement {
    let ui_colors = *colors;
    let on_submit = callbacks.on_submit.clone();
    let on_clear = callbacks.on_clear.clone();

    let search_icon = Icon::new(IconName::Search)
        .small()
        .text_color(ui_colors.muted);

    let input = Input::new(search_input)
        .accessibility_id("search-query-input")
        .aria_label("Search Hacker News")
        .prefix(search_icon)
        .cleanable(true);

    let mut submit_btn = Button::new("search-submit-button")
        .primary()
        .small()
        .label("Search")
        .on_click(move |_, window, cx| {
            on_submit(window, cx);
        });
    if !is_mobile {
        submit_btn = submit_btn.tooltip("Run search");
    }

    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_2()
        .items_center()
        .bg(ui_colors.surface)
        .border_b_1()
        .border_color(ui_colors.border)
        .child(div().flex_1().min_w_0().child(input))
        .when(has_active_state, |this| {
            let mut clear_btn = div()
                .id("search-reset-button")
                .role(Role::Button)
                .aria_label("Reset search")
                .px_2()
                .py_1()
                .rounded(px(6.0))
                .cursor_pointer()
                .bg(ui_colors.surface_2)
                .border_1()
                .border_color(ui_colors.border)
                .text_xs()
                .text_color(ui_colors.muted)
                .hover(move |s| s.bg(ui_colors.hover).text_color(ui_colors.text))
                .on_click(move |_, window, cx| {
                    on_clear(window, cx);
                })
                .child("Reset");
            if is_mobile {
                clear_btn = clear_btn.px_2p5().py_1p5();
            }
            this.child(clear_btn)
        })
        .child(submit_btn)
        .into_any_element()
}

fn render_filter_sort_toolbar(
    active_filter: SearchFilter,
    sort: SearchSort,
    results: &LoadState<SearchResults>,
    colors: &UiColors,
    callbacks: &SearchCallbacks,
) -> AnyElement {
    let ui_colors = *colors;
    let on_toggle_sort = callbacks.on_toggle_sort.clone();
    let hit_count = results.data().map(|res| res.nb_hits);

    let filter_chips =
        h_flex()
            .gap_1p5()
            .items_center()
            .children(SearchFilter::ALL.into_iter().map(|item| {
                let is_active = item == active_filter;
                let on_set_filter = callbacks.on_set_filter.clone();
                let chip_id = match item {
                    SearchFilter::All => "search-filter-all",
                    SearchFilter::Stories => "search-filter-stories",
                    SearchFilter::Comments => "search-filter-comments",
                };

                div()
                    .id(chip_id)
                    .role(Role::Button)
                    .aria_label(format!("Filter search: {}", item.label()))
                    .aria_selected(is_active)
                    .px_2p5()
                    .py_1()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_xs()
                    .font_medium()
                    .border_1()
                    .when(is_active, |this| {
                        this.bg(ui_colors.accent)
                            .border_color(ui_colors.accent)
                            .text_color(ui_colors.accent_fg)
                            .font_semibold()
                    })
                    .when(!is_active, |this| {
                        this.bg(ui_colors.surface_2)
                            .border_color(ui_colors.border)
                            .text_color(ui_colors.muted)
                            .hover(move |s| s.bg(ui_colors.hover).text_color(ui_colors.text))
                    })
                    .on_click(move |_, window, cx| {
                        on_set_filter(item, window, cx);
                    })
                    .child(item.label())
            }));

    let sort_button = div()
        .id("search-sort-toggle")
        .role(Role::Button)
        .aria_label(format!("Search sort: {}", sort.label()))
        .px_2p5()
        .py_1()
        .rounded(px(6.0))
        .cursor_pointer()
        .bg(ui_colors.surface_2)
        .border_1()
        .border_color(ui_colors.border)
        .text_xs()
        .font_medium()
        .text_color(ui_colors.text)
        .hover(move |s| s.bg(ui_colors.hover))
        .on_click(move |_, window, cx| {
            on_toggle_sort(window, cx);
        })
        .child(format!("Sort: {}", sort.label()));

    let right_controls = h_flex()
        .gap_2()
        .items_center()
        .when_some(hit_count, |this, nb_hits| {
            this.child(
                div()
                    .text_xs()
                    .text_color(ui_colors.subtle)
                    .child(format!("{nb_hits} hits")),
            )
        })
        .child(sort_button);

    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_2()
        .flex_wrap()
        .items_center()
        .justify_between()
        .bg(ui_colors.surface)
        .border_b_1()
        .border_color(ui_colors.border)
        .child(filter_chips)
        .child(right_controls)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_results_body(
    results: &LoadState<SearchResults>,
    hits: Arc<Vec<PreparedSearchHit>>,
    loading_more: bool,
    refreshing: bool,
    selected_story_id: Option<u64>,
    colors: &UiColors,
    list_state: &ListState,
    smooth_scroll_state: &SmoothScrollState,
    pull_to_refresh_state: &PullToRefreshState,
    smooth_scrolling: bool,
    is_mobile: bool,
    now_unix: i64,
    callbacks: &SearchCallbacks,
) -> AnyElement {
    let ui_colors = *colors;

    if hits.is_empty() {
        let content: AnyElement = match results {
            LoadState::Idle => render_idle_state(&ui_colors),
            LoadState::Loading => render_loading_state(&ui_colors),
            LoadState::Error(msg) => {
                render_error_state(msg, &ui_colors, callbacks.on_submit.clone())
            }
            LoadState::Loaded(_) => render_empty_results_state(&ui_colors),
        };

        let empty_scroll = div()
            .id("search-results-empty")
            .size_full()
            .overflow_y_scroll()
            .child(content);

        let wrapped_empty = if results.is_idle() {
            empty_scroll.into_any_element()
        } else {
            pull_to_refresh(
                "search-empty-ptr",
                empty_scroll,
                PullToRefreshTarget::AlwaysAtTop,
                pull_to_refresh_state,
                refreshing,
                &ui_colors,
                callbacks.on_refresh.clone(),
            )
            .into_any_element()
        };

        return div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(wrapped_empty)
            .into_any_element();
    }

    let inline_error: Option<String> = results.error().map(str::to_string);
    let has_more_pages = results
        .data()
        .is_some_and(|res| res.page + 1 < res.nb_pages);
    let hits_len = hits.len();
    let callbacks_cloned = callbacks.clone();

    let list_el = gpui::list(list_state.clone(), move |ix, _window, _cx| {
        let inner = if ix < hits_len {
            let prepared = &hits[ix];
            match &prepared.hit {
                SearchHit::Story(story) => render_story_hit_card(
                    ix,
                    story,
                    selected_story_id == Some(story.id),
                    &ui_colors,
                    is_mobile,
                    now_unix,
                    &callbacks_cloned,
                ),
                SearchHit::Comment(comment) => render_comment_hit_card(
                    ix,
                    comment,
                    prepared.excerpt.as_deref().unwrap_or(""),
                    selected_story_id == Some(comment.story_id),
                    &ui_colors,
                    now_unix,
                    &callbacks_cloned,
                ),
            }
        } else if ix == hits_len
            && let Some(err_msg) = &inline_error
        {
            render_inline_error_banner(err_msg, &ui_colors, callbacks_cloned.on_submit.clone())
        } else if has_more_pages {
            render_pagination_footer(
                loading_more,
                &ui_colors,
                callbacks_cloned.on_load_more.clone(),
            )
        } else {
            div().into_any_element()
        };

        div().w_full().px_2().pb_2().child(inner).into_any_element()
    })
    .size_full()
    .py_2();

    let smooth_list = smooth_scroll(
        list_el,
        SmoothScrollTarget::List(list_state.clone()),
        smooth_scroll_state,
        smooth_scrolling,
    );

    let pull_list = pull_to_refresh(
        "search-pull-to-refresh",
        smooth_list,
        PullToRefreshTarget::List(list_state.clone()),
        pull_to_refresh_state,
        refreshing,
        &ui_colors,
        callbacks.on_refresh.clone(),
    );

    div()
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .child(pull_list)
        .vertical_scrollbar(list_state)
        .into_any_element()
}

fn render_idle_state(colors: &UiColors) -> AnyElement {
    let ui_colors = *colors;
    v_flex()
        .w_full()
        .py(px(48.0))
        .px_4()
        .gap_2()
        .items_center()
        .justify_center()
        .child(
            Icon::new(IconName::Search)
                .large()
                .text_color(ui_colors.subtle),
        )
        .child(
            div()
                .text_sm()
                .text_color(ui_colors.muted)
                .child("Search stories & comments via Algolia HN"),
        )
        .into_any_element()
}

fn render_loading_state(colors: &UiColors) -> AnyElement {
    let ui_colors = *colors;
    v_flex()
        .w_full()
        .py(px(48.0))
        .px_4()
        .gap_2()
        .items_center()
        .justify_center()
        .child(Spinner::new().color(ui_colors.accent))
        .child(
            div()
                .text_sm()
                .text_color(ui_colors.muted)
                .child("Searching Hacker News…"),
        )
        .into_any_element()
}

fn render_error_state(msg: &str, colors: &UiColors, on_retry: ActionCallback) -> AnyElement {
    let ui_colors = *colors;
    v_flex()
        .w_full()
        .py(px(40.0))
        .px_4()
        .gap_3()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_sm()
                .text_color(ui_colors.text)
                .child(format!("Search error: {msg}")),
        )
        .child(
            div()
                .id("search-error-retry")
                .role(Role::Button)
                .aria_label("Retry search")
                .px_3()
                .py_1p5()
                .rounded(px(6.0))
                .cursor_pointer()
                .bg(ui_colors.accent)
                .text_xs()
                .font_semibold()
                .text_color(ui_colors.accent_fg)
                .on_click(move |_, window, cx| {
                    on_retry(window, cx);
                })
                .child("Retry"),
        )
        .into_any_element()
}

fn render_empty_results_state(colors: &UiColors) -> AnyElement {
    let ui_colors = *colors;
    v_flex()
        .w_full()
        .py(px(48.0))
        .px_4()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_sm()
                .text_color(ui_colors.muted)
                .child("No matching results found."),
        )
        .into_any_element()
}

fn render_inline_error_banner(
    err_msg: &str,
    colors: &UiColors,
    on_retry: ActionCallback,
) -> AnyElement {
    let ui_colors = *colors;
    h_flex()
        .w_full()
        .p_2()
        .gap_2()
        .items_center()
        .justify_between()
        .rounded(px(6.0))
        .bg(ui_colors.surface_2)
        .border_1()
        .border_color(ui_colors.border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(ui_colors.muted)
                .child(format!("Error refreshing search: {err_msg}")),
        )
        .child(
            div()
                .id("search-inline-retry")
                .role(Role::Button)
                .aria_label("Retry search refresh")
                .px_2p5()
                .py_1()
                .rounded(px(4.0))
                .cursor_pointer()
                .bg(ui_colors.accent)
                .text_xs()
                .font_semibold()
                .text_color(ui_colors.accent_fg)
                .on_click(move |_, window, cx| {
                    on_retry(window, cx);
                })
                .child("Retry"),
        )
        .into_any_element()
}

fn render_story_hit_card(
    idx: usize,
    story: &Story,
    is_selected: bool,
    colors: &UiColors,
    is_mobile: bool,
    now_unix: i64,
    callbacks: &SearchCallbacks,
) -> AnyElement {
    let ui_colors = *colors;
    let story_id = story.id;
    let title = story.title.clone();
    let domain = story.display_domain();
    let rel_time = story.relative_time(now_unix);
    let meta_text = format!("{} pts · by {} · {}", story.points, story.author, rel_time);
    let comments_label = format!("{} cmt", story.num_comments);
    let target_url = story.effective_url();
    let has_external_url = story
        .url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());

    let on_select_story = callbacks.on_select_story.clone();
    let on_open_url = callbacks.on_open_url.clone();

    let comments_pill = div()
        .px_2()
        .py(px(2.0))
        .rounded(px(999.0))
        .bg(ui_colors.surface_2)
        .border_1()
        .border_color(ui_colors.border)
        .text_xs()
        .font_medium()
        .text_color(ui_colors.muted)
        .child(comments_label);

    v_flex()
        .id(("search-hit-story", idx))
        .role(Role::Button)
        .aria_label(format!("Story: {title}"))
        .aria_selected(is_selected)
        .w_full()
        .p_3()
        .gap_1p5()
        .rounded(px(8.0))
        .cursor_pointer()
        .border_1()
        .when(is_selected, |this| {
            this.bg(ui_colors.accent_soft)
                .border_color(ui_colors.accent)
        })
        .when(!is_selected, |this| {
            this.bg(ui_colors.surface)
                .border_color(ui_colors.border)
                .hover(move |s| s.bg(ui_colors.hover))
        })
        .on_click(move |_, window, cx| {
            on_select_story(story_id, window, cx);
        })
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .items_start()
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_semibold()
                        .text_color(ui_colors.text)
                        .child(title),
                )
                .when(has_external_url, |row| {
                    let mut ext_link_btn = Button::new(("search-story-ext-link", idx))
                        .ghost()
                        .when(is_mobile, |btn| btn.small())
                        .when(!is_mobile, |btn| btn.xsmall())
                        .icon(IconName::ExternalLink)
                        .accessibility_label("Open external link")
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            on_open_url(target_url.clone(), window, cx);
                        });
                    if !is_mobile {
                        ext_link_btn = ext_link_btn.tooltip("Open link in browser");
                    }
                    row.child(
                        div()
                            .flex_shrink_0()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .child(ext_link_btn),
                    )
                }),
        )
        .child(div().text_xs().text_color(ui_colors.link).child(domain))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(ui_colors.muted)
                        .child(meta_text),
                )
                .child(comments_pill),
        )
        .into_any_element()
}

fn render_comment_hit_card(
    idx: usize,
    comment: &SearchCommentHit,
    excerpt: &str,
    is_selected: bool,
    colors: &UiColors,
    now_unix: i64,
    callbacks: &SearchCallbacks,
) -> AnyElement {
    let ui_colors = *colors;
    let story_id = comment.story_id;
    let rel_time = comment.relative_time(now_unix);
    let author_line = format!("{} · {}", comment.author, rel_time);
    let story_context = if comment.story_title.trim().is_empty() {
        format!("on story #{story_id}")
    } else {
        format!("on \"{}\"", comment.story_title)
    };
    let excerpt_display = if excerpt.is_empty() {
        "(no comment text)".to_string()
    } else {
        excerpt.to_string()
    };

    let on_select_story = callbacks.on_select_story.clone();

    v_flex()
        .id(("search-hit-comment", idx))
        .role(Role::Button)
        .aria_label(format!("Comment by {} {}", comment.author, story_context))
        .aria_selected(is_selected)
        .w_full()
        .p_3()
        .gap_1p5()
        .rounded(px(8.0))
        .cursor_pointer()
        .border_1()
        .when(is_selected, |this| {
            this.bg(ui_colors.accent_soft)
                .border_color(ui_colors.accent)
        })
        .when(!is_selected, |this| {
            this.bg(ui_colors.surface)
                .border_color(ui_colors.border)
                .hover(move |s| s.bg(ui_colors.hover))
        })
        .on_click(move |_, window, cx| {
            on_select_story(story_id, window, cx);
        })
        .child(
            h_flex()
                .w_full()
                .gap_1p5()
                .flex_wrap()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(ui_colors.accent)
                        .child(author_line),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(ui_colors.muted)
                        .child(story_context),
                ),
        )
        .child(
            div()
                .w_full()
                .text_sm()
                .text_color(ui_colors.text)
                .child(excerpt_display),
        )
        .into_any_element()
}

fn render_pagination_footer(
    loading_more: bool,
    colors: &UiColors,
    on_load_more: ActionCallback,
) -> AnyElement {
    let ui_colors = *colors;
    div()
        .w_full()
        .py_2()
        .flex()
        .items_center()
        .justify_center()
        .child(
            h_flex()
                .id("search-load-more-button")
                .role(Role::Button)
                .aria_label("Load more search results")
                .px_4()
                .py_2()
                .gap_2()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(ui_colors.border)
                .bg(ui_colors.surface_2)
                .text_xs()
                .font_semibold()
                .text_color(ui_colors.text)
                .when(!loading_more, |this| {
                    this.cursor_pointer()
                        .hover(move |s| s.bg(ui_colors.hover))
                        .on_click(move |_, window, cx| {
                            on_load_more(window, cx);
                        })
                })
                .when(loading_more, |this| {
                    this.opacity(0.7)
                        .child(Spinner::new().small().color(ui_colors.accent))
                })
                .child(if loading_more {
                    "Loading more results…"
                } else {
                    "Load more results"
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_list_item_count_tracks_error_and_pagination_slots() {
        assert_eq!(search_list_item_count(0, false, false), 0);
        assert_eq!(search_list_item_count(0, true, true), 0);
        assert_eq!(search_list_item_count(10, false, false), 10);
        assert_eq!(search_list_item_count(10, true, false), 11);
        assert_eq!(search_list_item_count(10, false, true), 11);
        assert_eq!(search_list_item_count(10, true, true), 12);
    }
}
