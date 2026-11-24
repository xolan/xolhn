use std::sync::Arc;

use super::{ActionCallback, IdCallback, UrlCallback, comment_tree};

use gpui::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement as _, ListState,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, div, list,
    prelude::FluentBuilder as _, px, relative, rems,
};
use gpui_kit::component::{
    IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    v_flex,
};
use xolhn_core::html::ContentBlock;
use xolhn_core::model::{CommentSort, Story, StoryThread};
use xolhn_core::settings::AppSettings;

use crate::pull_to_refresh::{PullToRefreshState, PullToRefreshTarget, pull_to_refresh};
use crate::smooth_scroll::{SmoothScrollState, SmoothScrollTarget, smooth_scroll};
use crate::state::{LoadState, VisibleCommentRow};
use crate::theme_bridge::UiColors;

/// Number of non-comment header items prepended to the virtualized reader [`gpui::list`].
pub const READER_HEADER_ITEM_COUNT: usize = 1;

/// Returns the total item count for the virtualized reader [`gpui::list`].
pub fn reader_list_item_count(
    has_story: bool,
    visible_comment_count: usize,
    has_error: bool,
    is_mobile: bool,
) -> usize {
    if !has_story {
        return 0;
    }
    if visible_comment_count == 0 {
        READER_HEADER_ITEM_COUNT + 1
    } else {
        READER_HEADER_ITEM_COUNT
            + visible_comment_count
            + usize::from(has_error)
            + usize::from(is_mobile)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReaderStatusSnapshot {
    Idle,
    Loading,
    Loaded,
    Error(String),
}

impl ReaderStatusSnapshot {
    fn from_load_state(state: &LoadState<StoryThread>) -> Self {
        match state {
            LoadState::Idle => Self::Idle,
            LoadState::Loading => Self::Loading,
            LoadState::Loaded(_) => Self::Loaded,
            LoadState::Error(err) => Self::Error(err.clone()),
        }
    }

    fn error(&self) -> Option<&str> {
        if let Self::Error(err) = self {
            Some(err.as_str())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReaderToolbarState {
    pub comment_count: usize,
    pub loaded_comment_count: usize,
    pub sort: CommentSort,
    pub all_collapsed: bool,
    pub refreshing: bool,
}

#[derive(Clone)]
pub struct ReaderCallbacks {
    pub on_open_url: UrlCallback,
    pub on_toggle_sort: ActionCallback,
    pub on_toggle_collapse_all: ActionCallback,
    pub on_refresh: ActionCallback,
    pub on_toggle_comment: IdCallback,
    pub on_retry: ActionCallback,
    pub on_back: Option<ActionCallback>,
    pub on_next_root: Option<ActionCallback>,
}

#[allow(clippy::too_many_arguments)]
pub fn render_reader_pane(
    story: Option<Arc<Story>>,
    thread_state: &LoadState<StoryThread>,
    selftext_blocks: Arc<Vec<ContentBlock>>,
    visible_comments: Arc<Vec<VisibleCommentRow>>,
    toolbar: &ReaderToolbarState,
    colors: &UiColors,
    settings: &AppSettings,
    list_state: &ListState,
    smooth_scroll_state: &SmoothScrollState,
    pull_to_refresh_state: &PullToRefreshState,
    is_mobile: bool,
    now_unix: i64,
    callbacks: ReaderCallbacks,
) -> AnyElement {
    let active_story = story.or_else(|| {
        thread_state
            .data()
            .map(|thread| Arc::new(thread.story.clone()))
    });
    let Some(story) = active_story else {
        return render_empty_prompt(colors, settings);
    };

    let status = ReaderStatusSnapshot::from_load_state(thread_state);
    let is_job = is_job_story(&story, thread_state);
    let total_comments = resolved_comment_count(&story, toolbar);

    if is_mobile {
        render_mobile_reader(
            story,
            status,
            visible_comments,
            *toolbar,
            selftext_blocks,
            is_job,
            total_comments,
            *colors,
            settings.clone(),
            list_state,
            smooth_scroll_state,
            pull_to_refresh_state,
            now_unix,
            callbacks,
        )
    } else {
        render_desktop_reader(
            story,
            status,
            visible_comments,
            *toolbar,
            selftext_blocks,
            is_job,
            total_comments,
            *colors,
            settings.clone(),
            list_state,
            smooth_scroll_state,
            pull_to_refresh_state,
            now_unix,
            callbacks,
        )
    }
}

fn render_empty_prompt(colors: &UiColors, settings: &AppSettings) -> AnyElement {
    v_flex()
        .id("reader-empty-state")
        .size_full()
        .min_w_0()
        .min_h_0()
        .bg(colors.surface)
        .text_color(colors.muted)
        .when_some(settings.ui_font_family.clone(), |this, font| {
            this.font_family(font)
        })
        .items_center()
        .justify_center()
        .p_6()
        .child(
            div()
                .text_size(rems(0.95))
                .text_color(colors.muted)
                .child("Select a story to read"),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_mobile_header_card(
    story: &Story,
    toolbar: &ReaderToolbarState,
    selftext_blocks: &[ContentBlock],
    total_comments: usize,
    colors: &UiColors,
    settings: &AppSettings,
    now_unix: i64,
    callbacks: &ReaderCallbacks,
    cx: &App,
) -> AnyElement {
    let domain = story.display_domain();
    let effective_url = story.effective_url();

    let on_open_banner = callbacks.on_open_url.clone();
    let link_banner = h_flex()
        .id("mobile-reader-link-banner")
        .role(Role::Link)
        .aria_label(format!("Open {domain} in browser"))
        .w_full()
        .px_2p5()
        .py_1p5()
        .gap_2()
        .items_center()
        .justify_between()
        .bg(colors.bg)
        .border_1()
        .border_color(colors.border)
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(rems(0.8))
        .text_color(colors.link)
        .when_some(settings.mono_font_family.clone(), |this, font| {
            this.font_family(font)
        })
        .on_click(move |_, window, cx| {
            (on_open_banner)(effective_url.clone(), window, cx);
        })
        .child(div().min_w_0().truncate().child(domain))
        .child(div().flex_shrink_0().child("Open in browser ↗"));

    let rel_time = story.relative_time(now_unix);
    let meta_line = if story.author.is_empty() {
        "Loading story details…".to_string()
    } else {
        format!(
            "{} pts · by {} · {} · {} comments",
            story.points, story.author, rel_time, total_comments
        )
    };

    let on_sort = callbacks.on_toggle_sort.clone();
    let sort_btn = Button::new("mobile-reader-sort")
        .xsmall()
        .outline()
        .compact()
        .label(format!("Sort: {}", toolbar.sort.label()))
        .on_click(move |_, window, cx| {
            (on_sort)(window, cx);
        });

    let on_collapse = callbacks.on_toggle_collapse_all.clone();
    let collapse_btn = Button::new("mobile-reader-collapse-all")
        .xsmall()
        .outline()
        .compact()
        .label(collapse_all_label(toolbar.all_collapsed))
        .on_click(move |_, window, cx| {
            (on_collapse)(window, cx);
        });

    let on_refresh = callbacks.on_refresh.clone();
    let refresh_btn = Button::new("mobile-reader-refresh")
        .xsmall()
        .outline()
        .compact()
        .icon(IconName::RefreshCw)
        .label("Refresh")
        .loading(toolbar.refreshing)
        .on_click(move |_, window, cx| {
            (on_refresh)(window, cx);
        });

    v_flex()
        .w_full()
        .px_3()
        .py_2p5()
        .gap_2()
        .bg(colors.surface)
        .border_b_1()
        .border_color(colors.border)
        .child(
            div()
                .text_size(rems(1.02))
                .line_height(relative(1.32))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.text)
                .child(story.title.clone()),
        )
        .child(link_banner)
        .child(
            div()
                .text_size(rems(0.78))
                .text_color(colors.muted)
                .child(meta_line),
        )
        .when(!selftext_blocks.is_empty(), |this| {
            this.child(
                div()
                    .w_full()
                    .px_2p5()
                    .py_2()
                    .bg(colors.bg)
                    .border_1()
                    .border_color(colors.border)
                    .rounded(px(6.))
                    .text_color(colors.text)
                    .child(comment_tree::render_html_blocks(
                        format!("mobile-story-selftext-{}", story.id),
                        selftext_blocks,
                        colors,
                        settings,
                        cx,
                    )),
            )
        })
        .child(
            h_flex()
                .w_full()
                .pt_1()
                .gap_1p5()
                .items_center()
                .child(sort_btn)
                .child(collapse_btn)
                .child(refresh_btn),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_mobile_reader(
    story: Arc<Story>,
    status: ReaderStatusSnapshot,
    visible_comments: Arc<Vec<VisibleCommentRow>>,
    toolbar: ReaderToolbarState,
    selftext_blocks: Arc<Vec<ContentBlock>>,
    is_job: bool,
    total_comments: usize,
    colors: UiColors,
    settings: AppSettings,
    list_state: &ListState,
    smooth_scroll_state: &SmoothScrollState,
    pull_to_refresh_state: &PullToRefreshState,
    now_unix: i64,
    callbacks: ReaderCallbacks,
) -> AnyElement {
    let domain = story.display_domain();
    let effective_url = story.effective_url();

    let on_back = callbacks.on_back.clone();
    let back_btn = Button::new("mobile-reader-back")
        .ghost()
        .small()
        .icon(IconName::ChevronLeft)
        .accessibility_label("Back to feed")
        .on_click(move |_, window, cx| {
            if let Some(handler) = &on_back {
                (handler)(window, cx);
            }
        });

    let on_open_top = callbacks.on_open_url.clone();
    let open_btn = Button::new("mobile-reader-open-url")
        .small()
        .outline()
        .compact()
        .icon(IconName::ExternalLink)
        .label("Open ↗")
        .on_click(move |_, window, cx| {
            (on_open_top)(effective_url.clone(), window, cx);
        });

    let top_bar = h_flex()
        .w_full()
        .h(px(44.))
        .px_2p5()
        .gap_2()
        .bg(colors.surface)
        .border_b_1()
        .border_color(colors.border)
        .items_center()
        .justify_between()
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .gap_1p5()
                .items_center()
                .child(back_btn)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(rems(0.92))
                        .font_weight(FontWeight::BOLD)
                        .text_color(colors.text)
                        .child(domain),
                ),
        )
        .child(open_btn);

    let has_comments = !visible_comments.is_empty();
    let on_next_root = callbacks.on_next_root.clone();
    let visible_len = visible_comments.len();
    let settings_for_list = settings.clone();
    let on_pull_refresh = callbacks.on_refresh.clone();

    let list_el = list(list_state.clone(), move |ix, _window, cx| {
        if ix == 0 {
            return render_mobile_header_card(
                &story,
                &toolbar,
                &selftext_blocks,
                total_comments,
                &colors,
                &settings_for_list,
                now_unix,
                &callbacks,
                cx,
            );
        }
        if visible_len == 0 {
            if ix == 1 {
                return div()
                    .w_full()
                    .px_3()
                    .child(render_comments_status(
                        &status,
                        is_job,
                        true,
                        &colors,
                        callbacks.on_retry.clone(),
                    ))
                    .into_any_element();
            }
            return div().into_any_element();
        }
        let comment_ix = ix - 1;
        if comment_ix < visible_len {
            return div()
                .w_full()
                .px_3()
                .child(comment_tree::render_comment_row(
                    &visible_comments[comment_ix],
                    &colors,
                    &settings_for_list,
                    now_unix,
                    true,
                    callbacks.on_toggle_comment.clone(),
                    cx,
                ))
                .into_any_element();
        }
        let tail_ix = comment_ix - visible_len;
        if let Some(err) = status.error() {
            if tail_ix == 0 {
                return div()
                    .w_full()
                    .px_3()
                    .child(render_error_notice(
                        err,
                        true,
                        &colors,
                        callbacks.on_retry.clone(),
                    ))
                    .into_any_element();
            }
            if tail_ix == 1 {
                // Clear the floating ↓ next-root-comment FAB at the bottom of the scroll list.
                return div().w_full().h(px(64.)).into_any_element();
            }
        } else if tail_ix == 0 {
            // Clear the floating ↓ next-root-comment FAB at the bottom of the scroll list.
            return div().w_full().h(px(64.)).into_any_element();
        }
        div().into_any_element()
    })
    .size_full();

    let smooth_list = smooth_scroll(
        list_el,
        SmoothScrollTarget::List(list_state.clone()),
        smooth_scroll_state,
        settings.smooth_scrolling,
    );

    let pull_list = pull_to_refresh(
        "mobile-reader-ptr",
        smooth_list,
        PullToRefreshTarget::List(list_state.clone()),
        pull_to_refresh_state,
        toolbar.refreshing,
        &colors,
        on_pull_refresh,
    );

    let mut body = div()
        .flex_1()
        .min_h_0()
        .relative()
        .overflow_hidden()
        .child(pull_list)
        .vertical_scrollbar(list_state);

    if has_comments && let Some(on_next_root) = on_next_root {
        body = body.child(
            div().absolute().right_4().bottom_4().child(
                Button::new("mobile-reader-next-root")
                    .primary()
                    .rounded(px(12.))
                    .with_size(px(36.))
                    .icon(IconName::ArrowDown)
                    .accessibility_label("Jump to next top-level comment")
                    .on_click(move |_, window, cx| {
                        (on_next_root)(window, cx);
                    }),
            ),
        );
    }

    v_flex()
        .id("mobile-reader-pane")
        .size_full()
        .min_w_0()
        .min_h_0()
        .bg(colors.surface)
        .text_color(colors.text)
        .when_some(settings.ui_font_family.clone(), |this, font| {
            this.font_family(font)
        })
        .child(top_bar)
        .child(body)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_desktop_header_and_toolbar(
    story: &Story,
    toolbar: &ReaderToolbarState,
    selftext_blocks: &[ContentBlock],
    total_comments: usize,
    colors: &UiColors,
    settings: &AppSettings,
    now_unix: i64,
    callbacks: &ReaderCallbacks,
    cx: &App,
) -> AnyElement {
    let effective_url = story.effective_url();
    let on_open_link = callbacks.on_open_url.clone();
    let open_link_btn = Button::new("desktop-reader-open-link")
        .primary()
        .small()
        .icon(IconName::ExternalLink)
        .label("Open Link ↗")
        .on_click(move |_, window, cx| {
            (on_open_link)(effective_url.clone(), window, cx);
        });

    let domain_pill = div()
        .px_1p5()
        .py_0p5()
        .bg(colors.surface_2)
        .border_1()
        .border_color(colors.border)
        .rounded(px(4.))
        .text_size(rems(0.76))
        .text_color(colors.link)
        .when_some(settings.mono_font_family.clone(), |this, font| {
            this.font_family(font)
        })
        .child(story.display_domain());

    let hn_thread_url = format!("https://news.ycombinator.com/item?id={}", story.id);
    let on_open_hn = callbacks.on_open_url.clone();
    let hn_thread_link = div()
        .id("desktop-reader-hn-thread-link")
        .role(Role::Link)
        .aria_label("Open Hacker News thread in browser")
        .cursor_pointer()
        .text_color(colors.subtle)
        .hover(|style| style.text_color(colors.link).underline())
        .on_click(move |_, window, cx| {
            (on_open_hn)(hn_thread_url.clone(), window, cx);
        })
        .child("· HN Thread ↗");

    let rel_time = story.relative_time(now_unix);

    let header_section = v_flex()
        .w_full()
        .pb_3()
        .mb_2()
        .gap_2()
        .border_b_1()
        .border_color(colors.border)
        .child(
            h_flex()
                .w_full()
                .items_start()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(rems(1.12))
                        .line_height(relative(1.3))
                        .font_weight(FontWeight::BOLD)
                        .text_color(colors.text)
                        .child(story.title.clone()),
                )
                .child(open_link_btn),
        )
        .child(
            h_flex()
                .w_full()
                .flex_wrap()
                .items_center()
                .gap_1p5()
                .text_size(rems(0.8))
                .text_color(colors.muted)
                .child(domain_pill)
                .when(!story.author.is_empty(), |this| {
                    this.child(div().child(format!("{} pts", story.points)))
                        .child(div().child(format!("· by {}", story.author)))
                        .child(div().child(format!("· {rel_time}")))
                })
                .when(story.author.is_empty(), |this| {
                    this.child(div().child("· Loading story details…"))
                })
                .child(hn_thread_link),
        )
        .when(!selftext_blocks.is_empty(), |this| {
            this.child(
                div()
                    .w_full()
                    .mt_1()
                    .px_3()
                    .py_2()
                    .bg(colors.bg)
                    .border_1()
                    .border_color(colors.border)
                    .rounded(px(6.))
                    .text_color(colors.text)
                    .child(comment_tree::render_html_blocks(
                        format!("desktop-story-selftext-{}", story.id),
                        selftext_blocks,
                        colors,
                        settings,
                        cx,
                    )),
            )
        });

    let on_sort = callbacks.on_toggle_sort.clone();
    let sort_btn = Button::new("desktop-reader-sort")
        .xsmall()
        .outline()
        .compact()
        .label(format!("Sort: {}", toolbar.sort.label()))
        .on_click(move |_, window, cx| {
            (on_sort)(window, cx);
        });

    let on_collapse = callbacks.on_toggle_collapse_all.clone();
    let collapse_btn = Button::new("desktop-reader-collapse-all")
        .xsmall()
        .outline()
        .compact()
        .label(collapse_all_label(toolbar.all_collapsed))
        .on_click(move |_, window, cx| {
            (on_collapse)(window, cx);
        });

    let on_refresh = callbacks.on_refresh.clone();
    let refresh_btn = Button::new("desktop-reader-refresh")
        .xsmall()
        .outline()
        .compact()
        .icon(IconName::RefreshCw)
        .label("Refresh")
        .loading(toolbar.refreshing)
        .on_click(move |_, window, cx| {
            (on_refresh)(window, cx);
        });

    let comments_toolbar = h_flex()
        .w_full()
        .py_1()
        .mb_1()
        .gap_2()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(rems(0.84))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.text)
                .child(format!("{total_comments} comments")),
        )
        .child(
            h_flex()
                .items_center()
                .gap_1p5()
                .child(sort_btn)
                .child(collapse_btn)
                .child(refresh_btn),
        );

    div()
        .w_full()
        .px_3p5()
        .child(
            v_flex()
                .w_full()
                .child(header_section)
                .child(comments_toolbar),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_desktop_reader(
    story: Arc<Story>,
    status: ReaderStatusSnapshot,
    visible_comments: Arc<Vec<VisibleCommentRow>>,
    toolbar: ReaderToolbarState,
    selftext_blocks: Arc<Vec<ContentBlock>>,
    is_job: bool,
    total_comments: usize,
    colors: UiColors,
    settings: AppSettings,
    list_state: &ListState,
    smooth_scroll_state: &SmoothScrollState,
    pull_to_refresh_state: &PullToRefreshState,
    now_unix: i64,
    callbacks: ReaderCallbacks,
) -> AnyElement {
    let visible_len = visible_comments.len();
    let settings_for_list = settings.clone();
    let on_pull_refresh = callbacks.on_refresh.clone();

    let list_el = list(list_state.clone(), move |ix, _window, cx| {
        if ix == 0 {
            return render_desktop_header_and_toolbar(
                &story,
                &toolbar,
                &selftext_blocks,
                total_comments,
                &colors,
                &settings_for_list,
                now_unix,
                &callbacks,
                cx,
            );
        }
        if visible_len == 0 {
            if ix == 1 {
                return div()
                    .w_full()
                    .px_3p5()
                    .child(render_comments_status(
                        &status,
                        is_job,
                        false,
                        &colors,
                        callbacks.on_retry.clone(),
                    ))
                    .into_any_element();
            }
            return div().into_any_element();
        }
        let comment_ix = ix - 1;
        if comment_ix < visible_len {
            return div()
                .w_full()
                .px_3p5()
                .child(comment_tree::render_comment_row(
                    &visible_comments[comment_ix],
                    &colors,
                    &settings_for_list,
                    now_unix,
                    false,
                    callbacks.on_toggle_comment.clone(),
                    cx,
                ))
                .into_any_element();
        }
        if comment_ix == visible_len
            && let Some(err) = status.error()
        {
            return div()
                .w_full()
                .px_3p5()
                .child(render_error_notice(
                    err,
                    false,
                    &colors,
                    callbacks.on_retry.clone(),
                ))
                .into_any_element();
        }
        div().into_any_element()
    })
    .size_full()
    .py_3();

    let smooth_list = smooth_scroll(
        list_el,
        SmoothScrollTarget::List(list_state.clone()),
        smooth_scroll_state,
        settings.smooth_scrolling,
    );

    let pull_list = pull_to_refresh(
        "desktop-reader-ptr",
        smooth_list,
        PullToRefreshTarget::List(list_state.clone()),
        pull_to_refresh_state,
        toolbar.refreshing,
        &colors,
        on_pull_refresh,
    );

    div()
        .id("desktop-reader-pane")
        .size_full()
        .min_w_0()
        .min_h_0()
        .relative()
        .overflow_hidden()
        .bg(colors.surface)
        .text_color(colors.text)
        .when_some(settings.ui_font_family.clone(), |this, font| {
            this.font_family(font)
        })
        .child(pull_list)
        .vertical_scrollbar(list_state)
        .into_any_element()
}

fn render_comments_status(
    status: &ReaderStatusSnapshot,
    is_job: bool,
    is_mobile: bool,
    colors: &UiColors,
    on_retry: ActionCallback,
) -> AnyElement {
    match status {
        ReaderStatusSnapshot::Loading => h_flex()
            .w_full()
            .py_6()
            .px_3()
            .items_center()
            .justify_center()
            .text_size(rems(0.85))
            .text_color(colors.muted)
            .child("Loading comments…")
            .into_any_element(),
        ReaderStatusSnapshot::Error(err) => render_error_notice(err, is_mobile, colors, on_retry),
        ReaderStatusSnapshot::Idle | ReaderStatusSnapshot::Loaded => div()
            .w_full()
            .py_3()
            .px_3()
            .text_size(rems(0.85))
            .text_color(colors.muted)
            .child(empty_comments_message(is_job))
            .into_any_element(),
    }
}

fn render_error_notice(
    err: &str,
    is_mobile: bool,
    colors: &UiColors,
    on_retry: ActionCallback,
) -> AnyElement {
    let retry_id = if is_mobile {
        "mobile-reader-retry"
    } else {
        "desktop-reader-retry"
    };

    v_flex()
        .w_full()
        .py_5()
        .px_3()
        .gap_2p5()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(rems(0.85))
                .text_color(colors.muted)
                .child(format!("Failed to load comments: {err}")),
        )
        .child(
            Button::new(retry_id)
                .small()
                .outline()
                .icon(IconName::RotateCw)
                .label("Retry")
                .on_click(move |_, window, cx| {
                    (on_retry)(window, cx);
                }),
        )
        .into_any_element()
}

fn is_job_story(story: &Story, thread_state: &LoadState<StoryThread>) -> bool {
    story.tags.iter().any(|tag| tag == "job")
        || thread_state
            .data()
            .is_some_and(|t| t.story.tags.iter().any(|tag| tag == "job"))
}

const fn empty_comments_message(is_job: bool) -> &'static str {
    if is_job {
        "Job listings on Hacker News have comments disabled."
    } else {
        "No comments yet."
    }
}

const fn collapse_all_label(all_collapsed: bool) -> &'static str {
    if all_collapsed {
        "Expand all"
    } else {
        "Collapse all"
    }
}

fn resolved_comment_count(story: &Story, toolbar: &ReaderToolbarState) -> usize {
    if toolbar.comment_count > 0 {
        toolbar.comment_count
    } else {
        story.num_comments.max(toolbar.loaded_comment_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_story(tags: Vec<&str>, num_comments: usize, text_html: Option<&str>) -> Story {
        Story {
            id: 421_901,
            title: "Sample Story".to_string(),
            url: Some("https://example.com/post".to_string()),
            domain: Some("example.com".to_string()),
            author: "alice".to_string(),
            points: 120,
            num_comments,
            created_at_i: 1_700_000_000,
            text_html: text_html.map(str::to_string),
            tags: tags.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn job_story_detection_and_empty_message() {
        let normal = sample_story(vec!["story"], 0, None);
        assert!(!is_job_story(&normal, &LoadState::Idle));
        assert_eq!(empty_comments_message(false), "No comments yet.");

        let job = sample_story(vec!["job"], 0, None);
        assert!(is_job_story(&job, &LoadState::Idle));
        assert_eq!(
            empty_comments_message(true),
            "Job listings on Hacker News have comments disabled."
        );
    }

    #[test]
    fn collapse_label_and_comment_count_resolution() {
        assert_eq!(collapse_all_label(false), "Collapse all");
        assert_eq!(collapse_all_label(true), "Expand all");

        let story = sample_story(vec!["story"], 42, None);
        let toolbar = ReaderToolbarState {
            comment_count: 0,
            loaded_comment_count: 15,
            sort: CommentSort::HnDefault,
            all_collapsed: false,
            refreshing: false,
        };
        assert_eq!(resolved_comment_count(&story, &toolbar), 42);

        let toolbar_explicit = ReaderToolbarState {
            comment_count: 55,
            ..toolbar
        };
        assert_eq!(resolved_comment_count(&story, &toolbar_explicit), 55);
    }

    #[test]
    fn reader_list_item_count_covers_desktop_mobile_empty_and_error_states() {
        assert_eq!(reader_list_item_count(false, 0, false, false), 0);
        assert_eq!(reader_list_item_count(false, 10, true, true), 0);

        // Empty/loading comments renders header + status item
        assert_eq!(reader_list_item_count(true, 0, false, false), 2);
        assert_eq!(reader_list_item_count(true, 0, true, true), 2);

        // Non-empty desktop: header + N + optional error
        assert_eq!(reader_list_item_count(true, 25, false, false), 26);
        assert_eq!(reader_list_item_count(true, 25, true, false), 27);

        // Non-empty mobile: header + N + optional error + FAB spacer
        assert_eq!(reader_list_item_count(true, 25, false, true), 27);
        assert_eq!(reader_list_item_count(true, 25, true, true), 28);
    }
}
