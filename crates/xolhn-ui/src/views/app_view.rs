use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, AsyncApp, Context, Entity, FocusHandle, Focusable,
    FontWeight, InteractiveElement as _, IntoElement, KeyBinding, ListAlignment, ListOffset,
    ListState, MouseButton, ParentElement as _, Pixels, Render, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, UniformListScrollHandle,
    WeakEntity, Window, div, list, px, rems, transparent_black,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex,
};
use xolhn_core::api::AlgoliaClient;
use xolhn_core::cache::{CacheConfig, CacheStore, CachedHnApi};
use xolhn_core::html::{ContentBlock, sanitize_url};
use xolhn_core::model::{
    FeedKind, FeedPage, SearchFilter, SearchHit, SearchResults, SearchSort, Story, StoryThread,
    current_unix_timestamp,
};
use xolhn_core::settings::{
    AppSettings, DEFAULT_FONT_SIZE_PX, LineHeightDensity, MAX_FONT_SIZE_PX, MIN_FONT_SIZE_PX,
    SettingsStore,
};
use xolhn_core::theme::ThemeId;

use crate::fonts;
use crate::pull_to_refresh::{PullToRefreshState, PullToRefreshTarget, pull_to_refresh};
use crate::smooth_scroll::{SmoothScrollState, SmoothScrollTarget, smooth_scroll};
use crate::state::{
    AppPaths, ChromeCallback, LoadState, MobileTab, PreparedSearchHit, PreparedThread,
    VisibleCommentRow, compute_comment_collapse_splice, flatten_comments_prepared,
    parse_selftext_blocks,
};
use crate::theme_bridge::{self, UiColors, rgba_hex};

use super::reader_pane::{self, ReaderCallbacks, ReaderToolbarState};
use super::search_view::{self, SearchCallbacks};
use super::settings_view::{self, SettingsCallbacks, SettingsScrollHandles};
use super::story_list;

const READER_KEY_CONTEXT: &str = "XolhnReader";
const READER_SHORTCUT_CONTEXT: &str = "XolhnReader && !Input";
const COMPACT_BREAKPOINT_PX: f32 = 760.0;
const MOBILE_FEED_CHIP_ORDER: [FeedKind; 6] = [
    FeedKind::Top,
    FeedKind::Best,
    FeedKind::New,
    FeedKind::Ask,
    FeedKind::Show,
    FeedKind::Jobs,
];

gpui::actions!(
    xolhn,
    [
        FocusSearch,
        NextStory,
        PrevStory,
        OpenSelectedLink,
        RefreshActive,
        CycleTheme,
        ToggleSettings,
        FeedTop,
        FeedNew,
        FeedBest,
        FeedAsk,
        FeedShow,
        FeedJobs,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        DismissOverlay,
    ]
);

/// Registers global and reader-scoped keyboard shortcuts for XolHN.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        // Global bindings (active everywhere, including while typing in search)
        KeyBinding::new("ctrl-k", FocusSearch, None),
        KeyBinding::new("cmd-k", FocusSearch, None),
        KeyBinding::new("ctrl-=", ZoomIn, None),
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("ctrl--", ZoomOut, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("ctrl-0", ZoomReset, None),
        KeyBinding::new("cmd-0", ZoomReset, None),
        KeyBinding::new("escape", DismissOverlay, None),
        // Single-key reader shortcuts (never hijack typing inside Input)
        KeyBinding::new("j", NextStory, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("down", NextStory, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("k", PrevStory, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("up", PrevStory, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("o", OpenSelectedLink, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("r", RefreshActive, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("t", CycleTheme, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new(",", ToggleSettings, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("/", FocusSearch, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("1", FeedTop, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("2", FeedNew, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("3", FeedBest, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("4", FeedAsk, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("5", FeedShow, Some(READER_SHORTCUT_CONTEXT)),
        KeyBinding::new("6", FeedJobs, Some(READER_SHORTCUT_CONTEXT)),
    ]);
}

/// Root XolHN application view coordinating feeds, thread reader, search, settings, and layouts.
pub struct XolhnApp {
    api: Arc<CachedHnApi<AlgoliaClient>>,
    settings_store: SettingsStore,
    settings: AppSettings,
    settings_save_lock: Arc<Mutex<()>>,
    settings_save_seq: Arc<AtomicU64>,
    chrome_cb: Option<ChromeCallback>,
    focus_handle: FocusHandle,
    ui_fonts: Arc<Vec<String>>,
    mono_fonts: Arc<Vec<String>>,

    // Feed state
    active_feed: FeedKind,
    feed_state: LoadState<FeedPage>,
    stories: Arc<Vec<Story>>,
    feed_page: usize,
    feed_nb_pages: usize,
    loading_more_feed: bool,
    feed_refreshing: bool,
    feed_req_id: u64,
    active_feed_task: Option<Task<()>>,

    // Thread state & precomputed reader caches
    selected_story_id: Option<u64>,
    selected_story_snapshot: Option<Arc<Story>>,
    thread_state: LoadState<StoryThread>,
    collapsed_ids: HashSet<u64>,
    all_collapsed: bool,
    thread_req_id: u64,
    thread_refreshing: bool,
    last_jumped_root_child: Option<usize>,
    active_thread_task: Option<Task<()>>,
    cached_visible_comments: Arc<Vec<VisibleCommentRow>>,
    cached_selftext_blocks: Arc<Vec<ContentBlock>>,
    cached_loaded_comment_count: usize,
    cached_all_comment_ids: Arc<Vec<u64>>,
    parsed_comment_blocks: HashMap<u64, Arc<Vec<ContentBlock>>>,
    comment_subtree_counts: HashMap<u64, usize>,

    // Search state
    search_input: Entity<InputState>,
    search_mode: bool,
    search_filter: SearchFilter,
    search_sort: SearchSort,
    search_state: LoadState<SearchResults>,
    search_hits: Arc<Vec<PreparedSearchHit>>,
    selected_search_index: Option<usize>,
    search_page: usize,
    search_nb_pages: usize,
    loading_more_search: bool,
    search_refreshing: bool,
    search_req_id: u64,
    active_search_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,

    // UI overlay / mobile state
    show_settings: bool,
    mobile_tab: MobileTab,
    mobile_showing_thread: bool,
    last_is_compact: Option<bool>,
    toast: Option<String>,
    toast_req_id: u64,

    // Virtualized list states & scroll handles
    feed_list_state: ListState,
    reader_list_state: ListState,
    search_list_state: ListState,
    settings_scroll: ScrollHandle,
    ui_fonts_scroll: UniformListScrollHandle,
    mono_fonts_scroll: UniformListScrollHandle,

    // Smooth-scroll & pull-to-refresh states
    feed_smooth_scroll: SmoothScrollState,
    reader_smooth_scroll: SmoothScrollState,
    search_smooth_scroll: SmoothScrollState,
    settings_smooth_scroll: SmoothScrollState,
    ui_fonts_smooth_scroll: SmoothScrollState,
    mono_fonts_smooth_scroll: SmoothScrollState,
    feed_pull_to_refresh: PullToRefreshState,
    reader_pull_to_refresh: PullToRefreshState,
    search_pull_to_refresh: PullToRefreshState,
}

impl XolhnApp {
    pub fn new(
        paths: AppPaths,
        chrome_cb: Option<ChromeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let cache = Arc::new(CacheStore::new(paths.cache_dir, CacheConfig::default()));
        let api = Arc::new(CachedHnApi::new(Arc::new(AlgoliaClient::new()), cache));

        let settings_store = SettingsStore::new(paths.config_dir);
        let settings = settings_store.load();
        let (ui_fonts, mono_fonts) = fonts::list_ui_and_mono_fonts(cx);

        theme_bridge::apply_theme_and_typography(&settings, Some(window), cx);
        if let Some(cb) = &chrome_cb {
            cb(settings.theme.palette());
        }

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search HN stories & comments..."));

        let search_sub = cx.subscribe_in(
            &search_input,
            window,
            |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let query = input.read(cx).value().trim().to_string();
                    this.focus_handle.focus(window, cx);
                    if query.is_empty() {
                        this.reset_search_results();
                        if !is_compact_layout(window) {
                            this.search_mode = false;
                        }
                        cx.notify();
                    } else {
                        if !is_compact_layout(window) {
                            this.search_mode = true;
                        } else {
                            this.mobile_tab = MobileTab::Search;
                            this.mobile_showing_thread = false;
                        }
                        this.run_search(0, false, false, cx);
                    }
                }
                InputEvent::Change => {
                    if input.read(cx).value().trim().is_empty() {
                        this.reset_search_results();
                        if !is_compact_layout(window) {
                            this.search_mode = false;
                        }
                        cx.notify();
                    }
                }
                InputEvent::Focus | InputEvent::Blur => {}
            },
        );

        let initial_compact = is_compact_layout(window);

        let mut app = Self {
            api,
            settings_store,
            settings,
            settings_save_lock: Arc::new(Mutex::new(())),
            settings_save_seq: Arc::new(AtomicU64::new(0)),
            chrome_cb,
            focus_handle,
            ui_fonts,
            mono_fonts,

            active_feed: FeedKind::Top,
            feed_state: LoadState::Idle,
            stories: Arc::new(Vec::new()),
            feed_page: 0,
            feed_nb_pages: 0,
            loading_more_feed: false,
            feed_refreshing: false,
            feed_req_id: 0,
            active_feed_task: None,

            selected_story_id: None,
            selected_story_snapshot: None,
            thread_state: LoadState::Idle,
            collapsed_ids: HashSet::new(),
            all_collapsed: false,
            thread_req_id: 0,
            thread_refreshing: false,
            last_jumped_root_child: None,
            active_thread_task: None,
            cached_visible_comments: Arc::new(Vec::new()),
            cached_selftext_blocks: Arc::new(Vec::new()),
            cached_loaded_comment_count: 0,
            cached_all_comment_ids: Arc::new(Vec::new()),
            parsed_comment_blocks: HashMap::new(),
            comment_subtree_counts: HashMap::new(),

            search_input,
            search_mode: false,
            search_filter: SearchFilter::default(),
            search_sort: SearchSort::default(),
            search_state: LoadState::Idle,
            search_hits: Arc::new(Vec::new()),
            selected_search_index: None,
            search_page: 0,
            search_nb_pages: 0,
            loading_more_search: false,
            search_refreshing: false,
            search_req_id: 0,
            active_search_task: None,
            _subscriptions: vec![search_sub],

            show_settings: false,
            mobile_tab: MobileTab::Feed,
            mobile_showing_thread: false,
            last_is_compact: Some(initial_compact),
            toast: None,
            toast_req_id: 0,

            feed_list_state: ListState::new(0, ListAlignment::Top, px(240.)),
            reader_list_state: ListState::new(0, ListAlignment::Top, px(320.)),
            search_list_state: ListState::new(0, ListAlignment::Top, px(240.)),
            settings_scroll: ScrollHandle::new(),
            ui_fonts_scroll: UniformListScrollHandle::new(),
            mono_fonts_scroll: UniformListScrollHandle::new(),

            feed_smooth_scroll: SmoothScrollState::new(),
            reader_smooth_scroll: SmoothScrollState::new(),
            search_smooth_scroll: SmoothScrollState::new(),
            settings_smooth_scroll: SmoothScrollState::new(),
            ui_fonts_smooth_scroll: SmoothScrollState::new(),
            mono_fonts_smooth_scroll: SmoothScrollState::new(),
            feed_pull_to_refresh: PullToRefreshState::new(),
            reader_pull_to_refresh: PullToRefreshState::new(),
            search_pull_to_refresh: PullToRefreshState::new(),
        };

        app.load_feed(FeedKind::Top, 0, false, false, cx);
        app
    }

    // -------------------------------------------------------------------------
    // Async Data Loading & State Transitions
    // -------------------------------------------------------------------------

    fn cancel_all_smooth_scrolls(&self) {
        self.feed_smooth_scroll.cancel();
        self.reader_smooth_scroll.cancel();
        self.search_smooth_scroll.cancel();
        self.settings_smooth_scroll.cancel();
        self.ui_fonts_smooth_scroll.cancel();
        self.mono_fonts_smooth_scroll.cancel();
    }

    fn current_is_mobile(&self) -> bool {
        self.last_is_compact.unwrap_or_else(gpui_kit::is_mobile)
    }

    fn expected_reader_item_count(&self, is_mobile: bool) -> usize {
        reader_pane::reader_list_item_count(
            self.active_story_ref().is_some(),
            self.cached_visible_comments.len(),
            self.thread_state.is_error(),
            is_mobile,
        )
    }

    fn expected_feed_item_count(&self) -> usize {
        if self.stories.is_empty() {
            0
        } else {
            self.stories.len() + 1
        }
    }

    fn expected_search_item_count(&self) -> usize {
        let has_more = self
            .search_state
            .data()
            .is_some_and(|res| res.page + 1 < res.nb_pages);
        search_view::search_list_item_count(
            self.search_hits.len(),
            self.search_state.is_error(),
            has_more,
        )
    }

    fn reset_reader_list_state(&self, is_mobile: bool) {
        self.reader_list_state
            .reset_with_uniform_height(self.expected_reader_item_count(is_mobile), px(110.));
    }

    fn rebuild_cached_visible_comments(&mut self) {
        let Some(thread) = self.thread_state.data() else {
            self.cached_visible_comments = Arc::new(Vec::new());
            return;
        };
        self.cached_visible_comments = Arc::new(flatten_comments_prepared(
            &thread.comments,
            &thread.story.author,
            self.settings.comment_sort,
            &self.collapsed_ids,
            &self.parsed_comment_blocks,
            &self.comment_subtree_counts,
        ));
    }

    fn load_feed(
        &mut self,
        feed: FeedKind,
        page: usize,
        append: bool,
        force_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        self.feed_req_id = self.feed_req_id.wrapping_add(1);
        let req_id = self.feed_req_id;
        self.active_feed = feed;

        if append {
            self.loading_more_feed = true;
            if !self.stories.is_empty() {
                let tail = self.stories.len();
                self.feed_list_state.remeasure_items(tail..(tail + 1));
            }
        } else {
            self.loading_more_feed = false;
            self.feed_refreshing = force_refresh && !self.stories.is_empty();
            if !force_refresh {
                self.stories = Arc::new(Vec::new());
                self.feed_page = 0;
                self.feed_nb_pages = 0;
                self.feed_smooth_scroll.cancel();
                self.feed_pull_to_refresh.reset();
                self.feed_list_state.reset(0);
            }
            if !force_refresh || self.stories.is_empty() {
                self.feed_state = LoadState::Loading;
            }
        }
        cx.notify();

        let api = Arc::clone(&self.api);
        let task = cx
            .background_executor()
            .spawn(async move { api.fetch_feed_with_options(feed, page, force_refresh) });

        self.active_feed_task = Some(cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let result = task.await;
                let _ = this.update(cx, |app, cx| {
                    if app.feed_req_id != req_id {
                        return;
                    }
                    app.loading_more_feed = false;
                    app.feed_refreshing = false;

                    match result {
                        Ok(feed_page) => {
                            app.feed_page = feed_page.page;
                            app.feed_nb_pages = feed_page.nb_pages;
                            let prev_len = app.stories.len();
                            let mut next_stories = if append {
                                (*app.stories).clone()
                            } else {
                                Vec::new()
                            };
                            append_unique_stories(&mut next_stories, &feed_page.stories);
                            app.stories = Arc::new(next_stories);
                            app.feed_state = LoadState::Loaded(feed_page);

                            let new_len = app.stories.len();
                            if append && prev_len > 0 && new_len > prev_len {
                                let added = new_len - prev_len;
                                app.feed_list_state
                                    .splice(prev_len..(prev_len + 1), added + 1);
                            } else if append && prev_len > 0 {
                                app.feed_list_state
                                    .remeasure_items(prev_len..(prev_len + 1));
                            } else {
                                app.feed_list_state.reset_with_uniform_height(
                                    app.expected_feed_item_count(),
                                    px(68.),
                                );
                            }

                            if !append
                                && page == 0
                                && app.selected_story_id.is_none()
                                && let Some(first) = app.stories.first().cloned()
                            {
                                app.select_story(first.id, Some(first), false, cx);
                            }
                        }
                        Err(err) => {
                            let msg = err.to_string();
                            if append && !app.stories.is_empty() {
                                let tail = app.stories.len();
                                app.feed_list_state.remeasure_items(tail..(tail + 1));
                                app.show_toast(format!("Could not load more: {msg}"), cx);
                            } else if force_refresh && !app.stories.is_empty() {
                                app.show_toast(format!("Refresh failed: {msg}"), cx);
                            } else {
                                app.feed_state = LoadState::Error(msg);
                            }
                        }
                    }
                    cx.notify();
                });
            },
        ));
    }

    fn load_thread(&mut self, story_id: u64, force_refresh: bool, cx: &mut Context<Self>) {
        self.thread_req_id = self.thread_req_id.wrapping_add(1);
        let req_id = self.thread_req_id;
        self.selected_story_id = Some(story_id);

        let refreshing_same = force_refresh
            && self
                .thread_state
                .data()
                .is_some_and(|thread| thread.story.id == story_id);

        let collapsed_snapshot = if refreshing_same {
            self.thread_refreshing = true;
            self.collapsed_ids.clone()
        } else {
            self.thread_refreshing = false;
            self.thread_state = LoadState::Loading;
            self.collapsed_ids.clear();
            self.all_collapsed = false;
            self.last_jumped_root_child = None;
            self.cached_visible_comments = Arc::new(Vec::new());
            self.cached_all_comment_ids = Arc::new(Vec::new());
            self.parsed_comment_blocks.clear();
            self.comment_subtree_counts.clear();
            self.cached_loaded_comment_count = 0;
            self.cached_selftext_blocks = parse_selftext_blocks(
                self.selected_story_snapshot
                    .as_deref()
                    .and_then(|s| s.text_html.as_deref()),
            );
            self.reader_smooth_scroll.cancel();
            self.reader_pull_to_refresh.reset();
            self.reset_reader_list_state(self.current_is_mobile());
            HashSet::new()
        };
        cx.notify();

        let api = Arc::clone(&self.api);
        let comment_sort = self.settings.comment_sort;
        let task = cx.background_executor().spawn(async move {
            let thread = api.fetch_thread_with_options(story_id, force_refresh)?;
            Ok::<PreparedThread, xolhn_core::api::HnError>(PreparedThread::build(
                thread,
                comment_sort,
                &collapsed_snapshot,
            ))
        });

        self.active_thread_task = Some(cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let result = task.await;
                let _ = this.update(cx, |app, cx| {
                    if app.thread_req_id != req_id {
                        return;
                    }
                    app.thread_refreshing = false;

                    match result {
                        Ok(mut prepared) => {
                            if let Some(snapshot) = &app.selected_story_snapshot
                                && snapshot.id == prepared.thread.story.id
                            {
                                if prepared.thread.story.title.is_empty() {
                                    prepared.thread.story.title = snapshot.title.clone();
                                }
                                if snapshot.tags.iter().any(|tag| tag == "job")
                                    && !prepared.thread.story.tags.iter().any(|tag| tag == "job")
                                {
                                    prepared.thread.story.tags.push("job".to_string());
                                }
                            }

                            app.selected_story_snapshot =
                                Some(Arc::new(prepared.thread.story.clone()));
                            app.cached_selftext_blocks = prepared.selftext_blocks;
                            app.parsed_comment_blocks = prepared.comment_blocks;
                            app.comment_subtree_counts = prepared.subtree_counts;
                            app.cached_all_comment_ids = prepared.all_comment_ids;
                            app.cached_loaded_comment_count = prepared.loaded_comment_count;

                            let reuse_prepared = app.settings.comment_sort == prepared.sort_used
                                && app.collapsed_ids == prepared.collapsed_used;
                            app.thread_state = LoadState::Loaded(prepared.thread);
                            if reuse_prepared {
                                app.cached_visible_comments = prepared.visible_comments;
                            } else {
                                app.rebuild_cached_visible_comments();
                            }
                            app.all_collapsed = !app.cached_all_comment_ids.is_empty()
                                && app
                                    .cached_all_comment_ids
                                    .iter()
                                    .all(|id| app.collapsed_ids.contains(id));
                            app.reset_reader_list_state(app.current_is_mobile());
                        }
                        Err(err) => {
                            let msg = err.to_string();
                            if refreshing_same {
                                app.show_toast(format!("Thread refresh failed: {msg}"), cx);
                            } else {
                                app.thread_state = LoadState::Error(msg);
                                app.reset_reader_list_state(app.current_is_mobile());
                            }
                        }
                    }
                    cx.notify();
                });
            },
        ));
    }

    fn run_search(
        &mut self,
        page: usize,
        append: bool,
        force_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let query = self.search_input.read(cx).value().trim().to_string();
        if query.is_empty() {
            self.reset_search_results();
            cx.notify();
            return;
        }

        self.search_req_id = self.search_req_id.wrapping_add(1);
        let req_id = self.search_req_id;
        let filter = self.search_filter;
        let sort = self.search_sort;

        if append {
            self.loading_more_search = true;
            let total = self.expected_search_item_count();
            if total > 0 {
                self.search_list_state.remeasure_items((total - 1)..total);
            }
        } else {
            self.loading_more_search = false;
            let refreshing_existing = force_refresh && !self.search_hits.is_empty();
            self.search_refreshing = refreshing_existing;
            if !refreshing_existing {
                self.search_hits = Arc::new(Vec::new());
                self.selected_search_index = None;
                self.search_page = 0;
                self.search_nb_pages = 0;
                self.search_state = LoadState::Loading;
                self.search_smooth_scroll.cancel();
                self.search_pull_to_refresh.reset();
                self.search_list_state.reset(0);
            }
        }
        cx.notify();

        let api = Arc::clone(&self.api);
        let task = cx.background_executor().spawn(async move {
            let results = api.search_with_options(&query, filter, sort, page, force_refresh)?;
            let prepared_hits: Vec<PreparedSearchHit> = results
                .hits
                .iter()
                .cloned()
                .map(PreparedSearchHit::from_hit)
                .collect();
            Ok::<(SearchResults, Vec<PreparedSearchHit>), xolhn_core::api::HnError>((
                results,
                prepared_hits,
            ))
        });

        self.active_search_task = Some(cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let result = task.await;
                let _ = this.update(cx, |app, cx| {
                    if app.search_req_id != req_id {
                        return;
                    }
                    app.loading_more_search = false;
                    app.search_refreshing = false;

                    match result {
                        Ok((results, prepared_hits)) => {
                            let prev_hit_count = app.search_hits.len();
                            let prev_total = app.expected_search_item_count();
                            app.search_page = results.page;
                            app.search_nb_pages = results.nb_pages;
                            if append {
                                let mut next_hits = (*app.search_hits).clone();
                                next_hits.extend(prepared_hits);
                                app.search_hits = Arc::new(next_hits);
                            } else {
                                app.search_hits = Arc::new(prepared_hits);
                            }
                            app.search_state = LoadState::Loaded(results);

                            let new_total = app.expected_search_item_count();
                            if append && prev_hit_count > 0 && prev_total >= prev_hit_count {
                                let new_tail = new_total.saturating_sub(prev_hit_count);
                                app.search_list_state
                                    .splice(prev_hit_count..prev_total, new_tail);
                            } else {
                                app.search_list_state
                                    .reset_with_uniform_height(new_total, px(96.));
                            }
                        }
                        Err(err) => {
                            let msg = err.to_string();
                            if append && !app.search_hits.is_empty() {
                                let total = app.expected_search_item_count();
                                if total > 0 {
                                    app.search_list_state.remeasure_items((total - 1)..total);
                                }
                                app.show_toast(format!("Could not load more results: {msg}"), cx);
                            } else if force_refresh && !app.search_hits.is_empty() {
                                app.show_toast(format!("Search refresh failed: {msg}"), cx);
                            } else {
                                app.search_state = LoadState::Error(msg);
                            }
                        }
                    }
                    cx.notify();
                });
            },
        ));
    }

    fn reset_search_results(&mut self) {
        self.search_req_id = self.search_req_id.wrapping_add(1);
        self.active_search_task = None;
        self.search_state = LoadState::Idle;
        self.search_hits = Arc::new(Vec::new());
        self.selected_search_index = None;
        self.search_page = 0;
        self.search_nb_pages = 0;
        self.loading_more_search = false;
        self.search_refreshing = false;
        self.search_smooth_scroll.cancel();
        self.search_pull_to_refresh.reset();
        self.search_list_state.reset(0);
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |input, cx| {
            input.clean(window, cx);
        });
        self.reset_search_results();
        if !is_compact_layout(window) {
            self.search_mode = false;
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    fn exit_search_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_mode = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn switch_feed(&mut self, feed: FeedKind, cx: &mut Context<Self>) {
        self.search_mode = false;
        self.mobile_tab = MobileTab::Feed;
        self.mobile_showing_thread = false;
        self.load_feed(feed, 0, false, false, cx);
    }

    fn select_story(
        &mut self,
        story_id: u64,
        snapshot: Option<Story>,
        open_mobile_thread: bool,
        cx: &mut Context<Self>,
    ) {
        let resolved_snapshot = snapshot.or_else(|| self.find_story_snapshot(story_id));
        if let Some(story) = resolved_snapshot {
            self.selected_story_snapshot = Some(Arc::new(story));
        }
        if open_mobile_thread {
            self.mobile_showing_thread = true;
        }
        self.load_thread(story_id, false, cx);
    }

    fn find_story_snapshot(&self, story_id: u64) -> Option<Story> {
        if let Some(story) = self.stories.iter().find(|s| s.id == story_id) {
            return Some(story.clone());
        }
        for prepared in self.search_hits.iter() {
            match &prepared.hit {
                SearchHit::Story(story) if story.id == story_id => return Some(story.clone()),
                SearchHit::Comment(comment) if comment.story_id == story_id => {
                    let domain = comment.story_url.as_deref().and_then(Story::extract_domain);
                    return Some(Story {
                        id: comment.story_id,
                        title: comment.story_title.clone(),
                        url: comment.story_url.clone(),
                        domain,
                        author: String::new(),
                        points: 0,
                        num_comments: 0,
                        created_at_i: 0,
                        text_html: None,
                        tags: vec!["story".to_string()],
                    });
                }
                _ => {}
            }
        }
        if let Some(thread) = self.thread_state.data()
            && thread.story.id == story_id
        {
            return Some(thread.story.clone());
        }
        None
    }

    fn active_story_ref(&self) -> Option<&Story> {
        self.thread_state
            .data()
            .map(|thread| &thread.story)
            .or(self.selected_story_snapshot.as_deref())
    }

    fn active_story_arc(&self) -> Option<Arc<Story>> {
        if let Some(snapshot) = &self.selected_story_snapshot {
            if let Some(thread) = self.thread_state.data()
                && thread.story.id != snapshot.id
            {
                return Some(Arc::new(thread.story.clone()));
            }
            return Some(Arc::clone(snapshot));
        }
        self.thread_state
            .data()
            .map(|thread| Arc::new(thread.story.clone()))
    }

    fn toggle_comment_collapse(&mut self, comment_id: u64, cx: &mut Context<Self>) {
        if !self.collapsed_ids.remove(&comment_id) {
            self.collapsed_ids.insert(comment_id);
        }
        self.last_jumped_root_child = None;
        self.all_collapsed = !self.cached_all_comment_ids.is_empty()
            && self
                .cached_all_comment_ids
                .iter()
                .all(|id| self.collapsed_ids.contains(id));

        let old_rows = Arc::clone(&self.cached_visible_comments);
        self.rebuild_cached_visible_comments();
        self.reader_smooth_scroll.cancel();
        let is_mobile = self.current_is_mobile();
        let expected_after = self.expected_reader_item_count(is_mobile);

        if let Some((old_range, new_count)) = compute_comment_collapse_splice(
            &old_rows,
            &self.cached_visible_comments,
            comment_id,
            reader_pane::READER_HEADER_ITEM_COUNT,
        ) {
            let expected_before = reader_pane::reader_list_item_count(
                self.active_story_ref().is_some(),
                old_rows.len(),
                self.thread_state.is_error(),
                is_mobile,
            );
            if self.reader_list_state.item_count() == expected_before
                && old_range.end <= expected_before
            {
                self.reader_list_state.splice(old_range, new_count);
            } else {
                self.reader_list_state
                    .reset_with_uniform_height(expected_after, px(110.));
            }
        } else {
            self.reader_list_state
                .reset_with_uniform_height(expected_after, px(110.));
        }
        cx.notify();
    }

    fn toggle_collapse_all(&mut self, cx: &mut Context<Self>) {
        if self.cached_all_comment_ids.is_empty() {
            return;
        }
        self.last_jumped_root_child = None;
        self.all_collapsed = !self.all_collapsed;
        if self.all_collapsed {
            self.collapsed_ids
                .extend(self.cached_all_comment_ids.iter().copied());
        } else {
            self.collapsed_ids.clear();
        }
        self.reader_smooth_scroll.cancel();
        self.rebuild_cached_visible_comments();
        self.reset_reader_list_state(self.current_is_mobile());
        cx.notify();
    }

    fn toggle_comment_sort(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.comment_sort = self.settings.comment_sort.toggle();
        self.last_jumped_root_child = None;
        let is_mobile = is_compact_layout(window);
        self.last_is_compact = Some(is_mobile);
        self.reader_smooth_scroll.cancel();
        self.rebuild_cached_visible_comments();
        self.reset_reader_list_state(is_mobile);
        let label = self.settings.comment_sort.label();
        self.apply_and_save_settings(Some(window), cx);
        self.show_toast(format!("Comments sorted by {label}"), cx);
    }

    fn jump_to_next_root_comment(&mut self, cx: &mut Context<Self>) {
        let header_offset = reader_pane::READER_HEADER_ITEM_COUNT;
        let top = self.reader_list_state.logical_scroll_top().item_ix;
        let max_y = self.reader_list_state.max_offset_for_scrollbar().y;
        let offset_y = self.reader_list_state.scroll_px_offset_for_scrollbar().y;
        let at_bottom = max_y > px(0.) && offset_y <= -max_y + px(4.);
        if let Some(child_idx) = next_root_comment_child_index(
            &self.cached_visible_comments,
            top,
            header_offset,
            self.last_jumped_root_child,
            at_bottom,
        ) {
            self.last_jumped_root_child = Some(child_idx);
            self.reader_smooth_scroll.cancel();
            self.reader_list_state.scroll_to(ListOffset {
                item_ix: child_idx,
                offset_in_item: px(0.),
            });
            cx.notify();
        }
    }

    fn open_external_url(&mut self, raw_url: String, cx: &mut Context<Self>) {
        let Some(safe_url) = sanitize_url(&raw_url) else {
            self.show_toast("Unsupported link scheme".to_string(), cx);
            return;
        };
        cx.open_url(&safe_url);
        let label = Story::extract_domain(&safe_url).unwrap_or_else(|| "link".to_string());
        self.show_toast(format!("Opening {label} in browser"), cx);
    }

    fn apply_and_save_settings(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.settings = self.settings.clone().clamped();
        theme_bridge::apply_theme_and_typography(&self.settings, window, cx);
        if let Some(cb) = &self.chrome_cb {
            cb(self.settings.theme.palette());
        }

        self.reader_list_state.remeasure();
        self.feed_list_state.remeasure();
        self.search_list_state.remeasure();

        let seq = self.settings_save_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let latest_seq = Arc::clone(&self.settings_save_seq);
        let save_lock = Arc::clone(&self.settings_save_lock);
        let store = self.settings_store.clone();
        let snapshot = self.settings.clone();
        cx.background_executor()
            .spawn(async move {
                let _guard = save_lock
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if latest_seq.load(Ordering::SeqCst) != seq {
                    return;
                }
                if let Err(err) = store.save(&snapshot) {
                    log::warn!("Failed to persist settings: {err}");
                }
            })
            .detach();

        cx.notify();
    }

    fn set_font_size(&mut self, size_px: f32, window: &mut Window, cx: &mut Context<Self>) {
        let clamped = size_px.clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX);
        self.settings.base_font_size_px = clamped;
        self.apply_and_save_settings(Some(window), cx);
    }

    fn show_toast(&mut self, msg: String, cx: &mut Context<Self>) {
        self.toast_req_id = self.toast_req_id.wrapping_add(1);
        let req_id = self.toast_req_id;
        self.toast = Some(msg);
        cx.notify();

        let timer = cx.background_executor().timer(Duration::from_secs(2));
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            timer.await;
            let _ = this.update(cx, |app, cx| {
                if app.toast_req_id == req_id {
                    app.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn has_more_feed_pages(&self) -> bool {
        feed_has_more_pages(
            self.active_feed,
            self.feed_page,
            self.feed_nb_pages,
            !self.stories.is_empty(),
        )
    }

    fn resolve_mono_font(&self, cx: &App) -> String {
        self.settings
            .mono_font_family
            .clone()
            .unwrap_or_else(|| cx.theme().mono_font_family.to_string())
    }

    // -------------------------------------------------------------------------
    // Keyboard Action Handlers
    // -------------------------------------------------------------------------

    fn on_focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        if is_compact_layout(window) {
            self.mobile_showing_thread = false;
            self.mobile_tab = MobileTab::Search;
        } else {
            if self.show_settings {
                self.cancel_all_smooth_scrolls();
            }
            self.show_settings = false;
        }
        self.search_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn on_next_story(&mut self, _: &NextStory, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_story_selection(1, cx);
    }

    fn on_prev_story(&mut self, _: &PrevStory, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_story_selection(-1, cx);
    }

    fn step_story_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.search_mode {
            if self.search_hits.is_empty() {
                return;
            }
            let current = self
                .selected_search_index
                .filter(|&idx| idx < self.search_hits.len())
                .or_else(|| {
                    self.selected_story_id.and_then(|id| {
                        self.search_hits
                            .iter()
                            .position(|prepared| search_hit_story_id(&prepared.hit) == id)
                    })
                });
            let next_idx = step_index(current, self.search_hits.len(), delta);
            self.selected_search_index = Some(next_idx);
            self.search_smooth_scroll.cancel();
            self.search_list_state.scroll_to_reveal_item(next_idx);
            let target_id = search_hit_story_id(&self.search_hits[next_idx].hit);
            let snapshot = self.find_story_snapshot(target_id);
            self.select_story(target_id, snapshot, false, cx);
            return;
        }

        if self.stories.is_empty() {
            return;
        }
        let current = self
            .selected_story_id
            .and_then(|id| self.stories.iter().position(|s| s.id == id));
        let next_idx = step_index(current, self.stories.len(), delta);
        let target = self.stories[next_idx].clone();
        self.feed_smooth_scroll.cancel();
        self.feed_list_state.scroll_to_reveal_item(next_idx);
        self.select_story(target.id, Some(target), false, cx);
    }

    fn on_open_selected_link(
        &mut self,
        _: &OpenSelectedLink,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(story) = self.active_story_ref() {
            let url = story.effective_url();
            self.open_external_url(url, cx);
        }
    }

    fn on_refresh_active(
        &mut self,
        _: &RefreshActive,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_active(cx);
    }

    fn refresh_active(&mut self, cx: &mut Context<Self>) {
        if self.search_mode {
            let query = self.search_input.read(cx).value().trim().to_string();
            if !query.is_empty() {
                self.run_search(0, false, true, cx);
                self.show_toast("Refreshing search…".to_string(), cx);
            }
            return;
        }

        let feed = self.active_feed;
        self.load_feed(feed, 0, false, true, cx);
        if let Some(story_id) = self.selected_story_id {
            self.load_thread(story_id, true, cx);
        }
        self.show_toast(format!("Refreshing {}…", feed.label()), cx);
    }

    fn on_cycle_theme(&mut self, _: &CycleTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_theme(window, cx);
    }

    fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next_theme = self.settings.theme.next();
        self.settings.theme = next_theme;
        self.apply_and_save_settings(Some(window), cx);
        self.show_toast(format!("Theme: {}", next_theme.display_name()), cx);
    }

    fn on_toggle_settings(
        &mut self,
        _: &ToggleSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_settings(window, cx);
    }

    fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if is_compact_layout(window) {
            self.mobile_showing_thread = false;
            self.mobile_tab = if self.mobile_tab == MobileTab::Settings {
                MobileTab::Feed
            } else {
                MobileTab::Settings
            };
        } else {
            self.cancel_all_smooth_scrolls();
            self.show_settings = !self.show_settings;
            if !self.show_settings {
                self.focus_handle.focus(window, cx);
            }
        }
        cx.notify();
    }

    fn on_feed_top(&mut self, _: &FeedTop, _window: &mut Window, cx: &mut Context<Self>) {
        self.switch_feed(FeedKind::Top, cx);
    }

    fn on_feed_new(&mut self, _: &FeedNew, _window: &mut Window, cx: &mut Context<Self>) {
        self.switch_feed(FeedKind::New, cx);
    }

    fn on_feed_best(&mut self, _: &FeedBest, _window: &mut Window, cx: &mut Context<Self>) {
        self.switch_feed(FeedKind::Best, cx);
    }

    fn on_feed_ask(&mut self, _: &FeedAsk, _window: &mut Window, cx: &mut Context<Self>) {
        self.switch_feed(FeedKind::Ask, cx);
    }

    fn on_feed_show(&mut self, _: &FeedShow, _window: &mut Window, cx: &mut Context<Self>) {
        self.switch_feed(FeedKind::Show, cx);
    }

    fn on_feed_jobs(&mut self, _: &FeedJobs, _window: &mut Window, cx: &mut Context<Self>) {
        self.switch_feed(FeedKind::Jobs, cx);
    }

    fn on_zoom_in(&mut self, _: &ZoomIn, window: &mut Window, cx: &mut Context<Self>) {
        let next =
            (self.settings.base_font_size_px + 1.0).clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX);
        self.set_font_size(next, window, cx);
        self.show_toast(format!("Font size: {next:.0} px"), cx);
    }

    fn on_zoom_out(&mut self, _: &ZoomOut, window: &mut Window, cx: &mut Context<Self>) {
        let next =
            (self.settings.base_font_size_px - 1.0).clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX);
        self.set_font_size(next, window, cx);
        self.show_toast(format!("Font size: {next:.0} px"), cx);
    }

    fn on_zoom_reset(&mut self, _: &ZoomReset, window: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(DEFAULT_FONT_SIZE_PX, window, cx);
        self.show_toast(
            format!("Font size: {DEFAULT_FONT_SIZE_PX:.0} px (default)"),
            cx,
        );
    }

    fn on_dismiss_overlay(
        &mut self,
        _: &DismissOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_settings {
            self.cancel_all_smooth_scrolls();
            self.show_settings = false;
            self.focus_handle.focus(window, cx);
            cx.notify();
            return;
        }
        if self.mobile_showing_thread {
            self.mobile_showing_thread = false;
            cx.notify();
            return;
        }
        if self.search_input.focus_handle(cx).is_focused(window) {
            self.focus_handle.focus(window, cx);
            return;
        }
        if self.search_mode {
            self.exit_search_mode(window, cx);
            return;
        }
        self.focus_handle.focus(window, cx);
    }

    // -------------------------------------------------------------------------
    // Child View Callbacks
    // -------------------------------------------------------------------------

    fn build_reader_callbacks(&self, is_mobile: bool, cx: &Context<Self>) -> ReaderCallbacks {
        let weak_open = cx.weak_entity();
        let weak_sort = cx.weak_entity();
        let weak_collapse_all = cx.weak_entity();
        let weak_refresh = cx.weak_entity();
        let weak_toggle_comment = cx.weak_entity();
        let weak_retry = cx.weak_entity();

        let on_back = if is_mobile {
            let weak_back = cx.weak_entity();
            Some(Rc::new(move |_window: &mut Window, cx: &mut App| {
                let _ = weak_back.update(cx, |app, cx| {
                    app.mobile_showing_thread = false;
                    cx.notify();
                });
            }) as Rc<dyn Fn(&mut Window, &mut App)>)
        } else {
            None
        };

        let on_next_root = if is_mobile {
            let weak_next = cx.weak_entity();
            Some(Rc::new(move |_window: &mut Window, cx: &mut App| {
                let _ = weak_next.update(cx, |app, cx| {
                    app.jump_to_next_root_comment(cx);
                });
            }) as Rc<dyn Fn(&mut Window, &mut App)>)
        } else {
            None
        };

        ReaderCallbacks {
            on_open_url: Rc::new(move |url, _window, cx| {
                let _ = weak_open.update(cx, |app, cx| {
                    app.open_external_url(url, cx);
                });
            }),
            on_toggle_sort: Rc::new(move |window, cx| {
                let _ = weak_sort.update(cx, |app, cx| {
                    app.toggle_comment_sort(window, cx);
                });
            }),
            on_toggle_collapse_all: Rc::new(move |_window, cx| {
                let _ = weak_collapse_all.update(cx, |app, cx| {
                    app.toggle_collapse_all(cx);
                });
            }),
            on_refresh: Rc::new(move |_window, cx| {
                let _ = weak_refresh.update(cx, |app, cx| {
                    if let Some(story_id) = app.selected_story_id {
                        app.load_thread(story_id, true, cx);
                    }
                });
            }),
            on_toggle_comment: Rc::new(move |comment_id, _window, cx| {
                let _ = weak_toggle_comment.update(cx, |app, cx| {
                    app.toggle_comment_collapse(comment_id, cx);
                });
            }),
            on_retry: Rc::new(move |_window, cx| {
                let _ = weak_retry.update(cx, |app, cx| {
                    if let Some(story_id) = app.selected_story_id {
                        app.load_thread(story_id, true, cx);
                    }
                });
            }),
            on_back,
            on_next_root,
        }
    }

    fn build_search_callbacks(&self, is_mobile: bool, cx: &Context<Self>) -> SearchCallbacks {
        let weak_submit = cx.weak_entity();
        let weak_refresh = cx.weak_entity();
        let weak_clear = cx.weak_entity();
        let weak_filter = cx.weak_entity();
        let weak_sort = cx.weak_entity();
        let weak_select = cx.weak_entity();
        let weak_open = cx.weak_entity();
        let weak_more = cx.weak_entity();

        SearchCallbacks {
            on_submit: Rc::new(move |window, cx| {
                let _ = weak_submit.update(cx, |app, cx| {
                    app.focus_handle.focus(window, cx);
                    app.run_search(0, false, false, cx);
                });
            }),
            on_refresh: Rc::new(move |_window, cx| {
                let _ = weak_refresh.update(cx, |app, cx| {
                    app.run_search(0, false, true, cx);
                });
            }),
            on_clear: Rc::new(move |window, cx| {
                let _ = weak_clear.update(cx, |app, cx| {
                    app.clear_search(window, cx);
                });
            }),
            on_set_filter: Rc::new(move |filter: SearchFilter, _window, cx| {
                let _ = weak_filter.update(cx, |app, cx| {
                    if app.search_filter != filter {
                        app.search_filter = filter;
                        if !app.search_input.read(cx).value().trim().is_empty() {
                            app.run_search(0, false, false, cx);
                        } else {
                            cx.notify();
                        }
                    }
                });
            }),
            on_toggle_sort: Rc::new(move |_window, cx| {
                let _ = weak_sort.update(cx, |app, cx| {
                    app.search_sort = app.search_sort.toggle();
                    if !app.search_input.read(cx).value().trim().is_empty() {
                        app.run_search(0, false, false, cx);
                    } else {
                        cx.notify();
                    }
                });
            }),
            on_select_story: Rc::new(move |story_id, _window, cx| {
                let _ = weak_select.update(cx, |app, cx| {
                    app.select_story(story_id, None, is_mobile, cx);
                });
            }),
            on_open_url: Rc::new(move |url, _window, cx| {
                let _ = weak_open.update(cx, |app, cx| {
                    app.open_external_url(url, cx);
                });
            }),
            on_load_more: Rc::new(move |_window, cx| {
                let _ = weak_more.update(cx, |app, cx| {
                    if !app.loading_more_search && app.search_page + 1 < app.search_nb_pages {
                        app.run_search(app.search_page + 1, true, false, cx);
                    }
                });
            }),
        }
    }

    fn build_settings_callbacks(&self, is_mobile: bool, cx: &Context<Self>) -> SettingsCallbacks {
        let weak_theme = cx.weak_entity();
        let weak_size = cx.weak_entity();
        let weak_lh = cx.weak_entity();
        let weak_ui_font = cx.weak_entity();
        let weak_mono_font = cx.weak_entity();
        let weak_smooth = cx.weak_entity();
        let weak_cache = cx.weak_entity();
        let weak_reset = cx.weak_entity();

        let on_close = if is_mobile {
            None
        } else {
            let weak_close = cx.weak_entity();
            Some(Rc::new(move |window: &mut Window, cx: &mut App| {
                let _ = weak_close.update(cx, |app, cx| {
                    app.cancel_all_smooth_scrolls();
                    app.show_settings = false;
                    app.focus_handle.focus(window, cx);
                    cx.notify();
                });
            }) as Rc<dyn Fn(&mut Window, &mut App)>)
        };

        SettingsCallbacks {
            on_select_theme: Rc::new(move |theme: ThemeId, window, cx| {
                let _ = weak_theme.update(cx, |app, cx| {
                    app.settings.theme = theme;
                    app.apply_and_save_settings(Some(window), cx);
                    app.show_toast(format!("Theme: {}", theme.display_name()), cx);
                });
            }),
            on_set_font_size: Rc::new(move |size_px: f32, window, cx| {
                let _ = weak_size.update(cx, |app, cx| {
                    app.set_font_size(size_px, window, cx);
                });
            }),
            on_set_line_height: Rc::new(move |density: LineHeightDensity, window, cx| {
                let _ = weak_lh.update(cx, |app, cx| {
                    app.settings.line_height_density = density;
                    app.apply_and_save_settings(Some(window), cx);
                });
            }),
            on_set_ui_font: Rc::new(move |font: Option<String>, window, cx| {
                let _ = weak_ui_font.update(cx, |app, cx| {
                    app.settings.ui_font_family = font;
                    app.apply_and_save_settings(Some(window), cx);
                });
            }),
            on_set_mono_font: Rc::new(move |font: Option<String>, window, cx| {
                let _ = weak_mono_font.update(cx, |app, cx| {
                    app.settings.mono_font_family = font;
                    app.apply_and_save_settings(Some(window), cx);
                });
            }),
            on_set_smooth_scrolling: Rc::new(move |enabled: bool, window, cx| {
                let _ = weak_smooth.update(cx, |app, cx| {
                    app.settings.smooth_scrolling = enabled;
                    if !enabled {
                        app.cancel_all_smooth_scrolls();
                    }
                    app.apply_and_save_settings(Some(window), cx);
                    app.show_toast(
                        if enabled {
                            "Smooth scrolling enabled"
                        } else {
                            "Smooth scrolling disabled"
                        }
                        .to_string(),
                        cx,
                    );
                });
            }),
            on_clear_cache: Rc::new(move |_window, cx| {
                let _ = weak_cache.update(cx, |app, cx| {
                    let api = Arc::clone(&app.api);
                    let task = cx
                        .background_executor()
                        .spawn(async move { api.cache().clear() });
                    cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                        let result = task.await;
                        let _ = this.update(cx, |app, cx| match result {
                            Ok(()) => app.show_toast("Cache cleared".to_string(), cx),
                            Err(err) => {
                                app.show_toast(format!("Failed to clear cache: {err}"), cx);
                            }
                        });
                    })
                    .detach();
                });
            }),
            on_reset_typography: Rc::new(move |window, cx| {
                let _ = weak_reset.update(cx, |app, cx| {
                    let defaults = AppSettings::default();
                    app.settings.ui_font_family = defaults.ui_font_family;
                    app.settings.mono_font_family = defaults.mono_font_family;
                    app.settings.base_font_size_px = defaults.base_font_size_px;
                    app.settings.line_height_density = defaults.line_height_density;
                    app.apply_and_save_settings(Some(window), cx);
                    app.show_toast("Typography reset to defaults".to_string(), cx);
                });
            }),
            on_close,
        }
    }

    // -------------------------------------------------------------------------
    // Desktop Layout Rendering
    // -------------------------------------------------------------------------

    fn render_desktop_layout(
        &self,
        colors: &UiColors,
        now_unix: i64,
        cx: &Context<Self>,
    ) -> AnyElement {
        let top_bar = self.render_desktop_top_bar(colors, cx);
        let left_rail = self.render_desktop_rail(colors, cx);
        let middle_pane = if self.search_mode {
            self.render_desktop_search_pane(colors, now_unix, cx)
        } else {
            self.render_desktop_feed_pane(colors, now_unix, cx)
        };
        let reader = self.render_reader(colors, false, now_unix, cx);

        let mut body = div()
            .flex()
            .flex_row()
            .items_stretch()
            .flex_1()
            .min_h_0()
            .w_full()
            .relative()
            .overflow_hidden()
            .child(left_rail)
            .child(middle_pane)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .h_full()
                    .bg(colors.surface)
                    .child(reader),
            );

        if self.show_settings {
            body = body.child(self.render_settings_drawer(colors, cx));
        }

        v_flex()
            .size_full()
            .min_h_0()
            .relative()
            .bg(colors.bg)
            .text_color(colors.text)
            .child(top_bar)
            .child(body)
            .when_some(self.toast.as_deref(), |this, msg| {
                this.child(render_toast_overlay(
                    msg,
                    colors,
                    false,
                    false,
                    crate::SafeAreaInsets::default(),
                ))
            })
            .into_any_element()
    }

    fn render_desktop_top_bar(&self, colors: &UiColors, cx: &Context<Self>) -> AnyElement {
        let ui_colors = *colors;
        let mono_font = self.resolve_mono_font(cx);

        let brand = h_flex()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .child(render_brand_mark(&ui_colors, 20.0))
            .child(
                div()
                    .text_size(rems(0.90))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_colors.text)
                    .child("XolHN"),
            );

        // Mount InputState in at most one location per frame: top bar when browsing feeds,
        // or inside search_view::render_search_panel when search_mode is active.
        let center_search: AnyElement = if self.search_mode {
            let query = self.search_input.read(cx).value();
            let summary = if query.trim().is_empty() {
                "Search active".to_string()
            } else {
                format!("Search: {}", query.trim())
            };
            let weak_focus = cx.weak_entity();
            let weak_exit = cx.weak_entity();

            h_flex()
                .flex_1()
                .max_w(px(360.))
                .h(px(28.))
                .px_2p5()
                .gap_2()
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .bg(ui_colors.bg)
                .border_1()
                .border_color(ui_colors.accent)
                .child(
                    h_flex()
                        .id("desktop-topbar-search-active")
                        .flex_1()
                        .min_w_0()
                        .gap_1p5()
                        .items_center()
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            let _ = weak_focus.update(cx, |app, cx| {
                                app.search_input.update(cx, |input, cx| {
                                    input.focus(window, cx);
                                });
                            });
                        })
                        .child(
                            Icon::new(IconName::Search)
                                .xsmall()
                                .text_color(ui_colors.accent),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_xs()
                                .text_color(ui_colors.text)
                                .child(summary),
                        ),
                )
                .child(
                    Button::new("desktop-topbar-exit-search")
                        .ghost()
                        .xsmall()
                        .compact()
                        .label("Feed")
                        .on_click(move |_, window, cx| {
                            let _ = weak_exit.update(cx, |app, cx| {
                                app.exit_search_mode(window, cx);
                            });
                        }),
                )
                .into_any_element()
        } else {
            let kbd_hint = div()
                .px_1p5()
                .py(px(1.))
                .rounded(px(3.))
                .bg(ui_colors.surface_2)
                .border_1()
                .border_color(ui_colors.border)
                .font_family(mono_font)
                .text_size(rems(0.68))
                .text_color(ui_colors.subtle)
                .whitespace_nowrap()
                .child("Ctrl+K");

            div()
                .flex_1()
                .max_w(px(360.))
                .min_w(px(160.))
                .child(
                    Input::new(&self.search_input)
                        .xsmall()
                        .accessibility_id("desktop-search-input")
                        .aria_label("Search Hacker News")
                        .prefix(
                            Icon::new(IconName::Search)
                                .xsmall()
                                .text_color(ui_colors.subtle),
                        )
                        .cleanable(true)
                        .suffix(kbd_hint),
                )
                .into_any_element()
        };

        let weak_refresh = cx.weak_entity();
        let weak_settings = cx.weak_entity();
        let weak_theme = cx.weak_entity();

        let theme_icon = if self.settings.theme.is_dark() {
            IconName::Sun
        } else {
            IconName::Moon
        };

        let right_actions = h_flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(
                Button::new("desktop-topbar-refresh")
                    .ghost()
                    .small()
                    .icon(IconName::RefreshCw)
                    .tooltip("Refresh (r)")
                    .accessibility_label("Refresh feed and thread")
                    .on_click(move |_, _window, cx| {
                        let _ = weak_refresh.update(cx, |app, cx| {
                            app.refresh_active(cx);
                        });
                    }),
            )
            .child(
                Button::new("desktop-topbar-settings")
                    .ghost()
                    .small()
                    .selected(self.show_settings)
                    .icon(IconName::Settings)
                    .tooltip("Settings (,)")
                    .accessibility_label("Toggle settings")
                    .on_click(move |_, window, cx| {
                        let _ = weak_settings.update(cx, |app, cx| {
                            app.toggle_settings(window, cx);
                        });
                    }),
            )
            .child(
                Button::new("desktop-topbar-theme")
                    .ghost()
                    .small()
                    .icon(theme_icon)
                    .tooltip(format!(
                        "Cycle theme (t) · {}",
                        self.settings.theme.display_name()
                    ))
                    .accessibility_label("Cycle color theme")
                    .on_click(move |_, window, cx| {
                        let _ = weak_theme.update(cx, |app, cx| {
                            app.cycle_theme(window, cx);
                        });
                    }),
            );

        h_flex()
            .w_full()
            .h(px(40.))
            .flex_shrink_0()
            .px_3()
            .gap_3()
            .items_center()
            .justify_between()
            .bg(ui_colors.surface)
            .border_b_1()
            .border_color(ui_colors.border)
            .child(brand)
            .child(center_search)
            .child(right_actions)
            .into_any_element()
    }

    fn render_desktop_rail(&self, colors: &UiColors, cx: &Context<Self>) -> AnyElement {
        let ui_colors = *colors;

        let mut feed_group = v_flex().w_full().items_center().gap_1();
        for feed in FeedKind::ALL {
            let is_active = !self.search_mode && self.active_feed == feed;
            let weak = cx.weak_entity();
            feed_group = feed_group.child(render_rail_button(
                SharedString::from(format!("rail-feed-{}", feed.slug())),
                feed_icon(feed),
                feed.short_label(),
                is_active,
                &ui_colors,
                Rc::new(move |_window, cx| {
                    let _ = weak.update(cx, |app, cx| {
                        app.switch_feed(feed, cx);
                    });
                }),
            ));
        }

        let weak_settings = cx.weak_entity();
        let bottom_group = v_flex()
            .w_full()
            .items_center()
            .gap_1()
            .child(render_rail_button(
                SharedString::from("rail-settings"),
                IconName::Settings,
                "Prefs",
                self.show_settings,
                &ui_colors,
                Rc::new(move |window, cx| {
                    let _ = weak_settings.update(cx, |app, cx| {
                        app.toggle_settings(window, cx);
                    });
                }),
            ));

        v_flex()
            .id("desktop-icon-rail")
            .w(px(54.))
            .h_full()
            .flex_shrink_0()
            .py_2()
            .bg(ui_colors.surface)
            .border_r_1()
            .border_color(ui_colors.border)
            .items_center()
            .justify_between()
            .child(feed_group)
            .child(bottom_group)
            .into_any_element()
    }

    fn render_desktop_feed_pane(
        &self,
        colors: &UiColors,
        now_unix: i64,
        cx: &Context<Self>,
    ) -> AnyElement {
        let ui_colors = *colors;
        let mono_font = self.resolve_mono_font(cx);
        let loaded_label = format!("{} loaded", self.stories.len());

        let header = h_flex()
            .w_full()
            .h(px(34.))
            .flex_shrink_0()
            .px_2p5()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(ui_colors.border)
            .child(
                div()
                    .text_size(rems(0.84))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_colors.text)
                    .child(self.active_feed.label()),
            )
            .child(
                div()
                    .text_size(rems(0.72))
                    .text_color(ui_colors.subtle)
                    .child(loaded_label),
            );

        let list_body = self.render_feed_scroll_body(&ui_colors, &mono_font, false, now_unix, cx);

        v_flex()
            .id("desktop-feed-pane")
            .w(px(320.))
            .h_full()
            .flex_shrink_0()
            .min_h_0()
            .bg(ui_colors.bg)
            .border_r_1()
            .border_color(ui_colors.border)
            .child(header)
            .child(list_body)
            .into_any_element()
    }

    fn render_desktop_search_pane(
        &self,
        colors: &UiColors,
        now_unix: i64,
        cx: &Context<Self>,
    ) -> AnyElement {
        let ui_colors = *colors;
        let weak_back = cx.weak_entity();

        let header = h_flex()
            .w_full()
            .h(px(34.))
            .flex_shrink_0()
            .px_2p5()
            .items_center()
            .justify_between()
            .bg(ui_colors.surface)
            .border_b_1()
            .border_color(ui_colors.border)
            .child(
                div()
                    .text_size(rems(0.84))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_colors.text)
                    .child("Search"),
            )
            .child(
                Button::new("desktop-search-back-to-feed")
                    .ghost()
                    .xsmall()
                    .compact()
                    .icon(IconName::ChevronLeft)
                    .label("Back to feed")
                    .on_click(move |_, window, cx| {
                        let _ = weak_back.update(cx, |app, cx| {
                            app.exit_search_mode(window, cx);
                        });
                    }),
            );

        let callbacks = self.build_search_callbacks(false, cx);
        let search_panel = search_view::render_search_panel(
            &self.search_input,
            self.search_filter,
            self.search_sort,
            &self.search_state,
            Arc::clone(&self.search_hits),
            self.loading_more_search,
            self.search_refreshing,
            self.selected_story_id,
            &ui_colors,
            &self.search_list_state,
            &self.search_smooth_scroll,
            &self.search_pull_to_refresh,
            self.settings.smooth_scrolling,
            false,
            now_unix,
            callbacks,
        );

        v_flex()
            .id("desktop-search-pane")
            .w(px(320.))
            .h_full()
            .flex_shrink_0()
            .min_h_0()
            .bg(ui_colors.bg)
            .border_r_1()
            .border_color(ui_colors.border)
            .child(header)
            .child(div().flex_1().min_h_0().w_full().child(search_panel))
            .into_any_element()
    }

    fn settings_scroll_handles(&self) -> SettingsScrollHandles<'_> {
        SettingsScrollHandles {
            scroll_handle: &self.settings_scroll,
            scroll_smooth: &self.settings_smooth_scroll,
            ui_fonts_scroll: &self.ui_fonts_scroll,
            ui_fonts_smooth: &self.ui_fonts_smooth_scroll,
            mono_fonts_scroll: &self.mono_fonts_scroll,
            mono_fonts_smooth: &self.mono_fonts_smooth_scroll,
        }
    }

    fn render_settings_drawer(&self, colors: &UiColors, cx: &Context<Self>) -> AnyElement {
        let ui_colors = *colors;
        let weak_dismiss = cx.weak_entity();
        let callbacks = self.build_settings_callbacks(false, cx);
        let panel = settings_view::render_settings_panel(
            &self.settings,
            Arc::clone(&self.ui_fonts),
            Arc::clone(&self.mono_fonts),
            &ui_colors,
            self.settings_scroll_handles(),
            false,
            callbacks,
            cx,
        );

        div()
            .id("settings-drawer-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba_hex(
                0x000000,
                if ui_colors.is_dark { 0.45 } else { 0.25 },
            ))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                let _ = weak_dismiss.update(cx, |app, cx| {
                    app.cancel_all_smooth_scrolls();
                    app.show_settings = false;
                    app.focus_handle.focus(window, cx);
                    cx.notify();
                });
            })
            .child(
                div()
                    .id("settings-drawer-panel")
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(420.))
                    .max_w_full()
                    .occlude()
                    .bg(ui_colors.surface)
                    .border_l_1()
                    .border_color(ui_colors.border)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(panel),
            )
            .into_any_element()
    }

    // -------------------------------------------------------------------------
    // Mobile / Compact Layout Rendering
    // -------------------------------------------------------------------------

    fn render_mobile_layout(
        &self,
        colors: &UiColors,
        now_unix: i64,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let ui_colors = *colors;
        let safe = crate::effective_safe_area(window, cx);
        let safe_top = px(safe.top);
        let safe_bottom = px(safe.bottom);
        let safe_left = px(safe.left);
        let safe_right = px(safe.right);
        let showing_thread = self.mobile_showing_thread && self.mobile_tab != MobileTab::Settings;

        let mut root = v_flex()
            .size_full()
            .min_h_0()
            .relative()
            .bg(ui_colors.bg)
            .text_color(ui_colors.text)
            .when(safe.top > 0.0, |this| {
                this.child(
                    div()
                        .id("mobile-safe-top")
                        .w_full()
                        .h(safe_top)
                        .flex_shrink_0()
                        .bg(ui_colors.surface),
                )
            });

        if showing_thread {
            let reader = self.render_reader(&ui_colors, true, now_unix, cx);
            root = root.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .bg(ui_colors.surface)
                    .pl(safe_left)
                    .pr(safe_right)
                    .child(reader),
            );
        } else {
            let body: AnyElement = match self.mobile_tab {
                MobileTab::Feed => self.render_mobile_feed_pane(&ui_colors, now_unix, cx),
                MobileTab::Search => {
                    let callbacks = self.build_search_callbacks(true, cx);
                    search_view::render_search_panel(
                        &self.search_input,
                        self.search_filter,
                        self.search_sort,
                        &self.search_state,
                        Arc::clone(&self.search_hits),
                        self.loading_more_search,
                        self.search_refreshing,
                        self.selected_story_id,
                        &ui_colors,
                        &self.search_list_state,
                        &self.search_smooth_scroll,
                        &self.search_pull_to_refresh,
                        self.settings.smooth_scrolling,
                        true,
                        now_unix,
                        callbacks,
                    )
                }
                MobileTab::Settings => {
                    let callbacks = self.build_settings_callbacks(true, cx);
                    settings_view::render_settings_panel(
                        &self.settings,
                        Arc::clone(&self.ui_fonts),
                        Arc::clone(&self.mono_fonts),
                        &ui_colors,
                        self.settings_scroll_handles(),
                        true,
                        callbacks,
                        cx,
                    )
                }
            };
            let bottom_nav = self.render_mobile_bottom_nav(&ui_colors, safe_left, safe_right, cx);

            root = root
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .pl(safe_left)
                        .pr(safe_right)
                        .child(body),
                )
                .child(bottom_nav);
        }

        root.when(safe.bottom > 0.0, |this| {
            this.child(
                div()
                    .id("mobile-safe-bottom")
                    .w_full()
                    .h(safe_bottom)
                    .flex_shrink_0()
                    .bg(ui_colors.surface),
            )
        })
        .when_some(self.toast.as_deref(), |this, msg| {
            this.child(render_toast_overlay(
                msg,
                &ui_colors,
                true,
                showing_thread,
                safe,
            ))
        })
        .into_any_element()
    }

    fn render_mobile_feed_pane(
        &self,
        colors: &UiColors,
        now_unix: i64,
        cx: &Context<Self>,
    ) -> AnyElement {
        let ui_colors = *colors;
        let mono_font = self.resolve_mono_font(cx);

        let mut chip_row = h_flex()
            .id("mobile-feed-chips")
            .w_full()
            .flex_shrink_0()
            .overflow_x_scroll()
            .px_2p5()
            .gap(px(2.))
            .items_stretch()
            .bg(ui_colors.surface)
            .border_b_1()
            .border_color(ui_colors.border);

        for feed in MOBILE_FEED_CHIP_ORDER {
            let is_active = self.active_feed == feed;
            let weak = cx.weak_entity();
            chip_row = chip_row.child(
                v_flex()
                    .id(SharedString::from(format!("mobile-chip-{}", feed.slug())))
                    .role(Role::Tab)
                    .aria_label(format!("Feed: {}", feed.label()))
                    .aria_selected(is_active)
                    .flex_1()
                    .min_w(px(44.))
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .on_click(move |_, _window, cx| {
                        let _ = weak.update(cx, |app, cx| {
                            app.switch_feed(feed, cx);
                        });
                    })
                    .child(
                        div()
                            .pt(px(6.))
                            .pb(px(7.))
                            .px_1()
                            .text_size(rems(0.80))
                            .whitespace_nowrap()
                            .when(is_active, |this| {
                                this.text_color(ui_colors.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                            })
                            .when(!is_active, |this| {
                                this.text_color(ui_colors.muted)
                                    .font_weight(FontWeight::MEDIUM)
                                    .hover(move |s| s.text_color(ui_colors.text))
                            })
                            .child(feed.short_label()),
                    )
                    .child(div().w(px(28.)).h(px(2.)).rounded_full().bg(if is_active {
                        ui_colors.accent
                    } else {
                        transparent_black()
                    })),
            );
        }

        let list_body = self.render_feed_scroll_body(&ui_colors, &mono_font, true, now_unix, cx);

        v_flex()
            .id("mobile-feed-pane")
            .size_full()
            .min_h_0()
            .bg(ui_colors.bg)
            .child(chip_row)
            .child(list_body)
            .into_any_element()
    }

    fn render_mobile_bottom_nav(
        &self,
        colors: &UiColors,
        safe_left: Pixels,
        safe_right: Pixels,
        cx: &Context<Self>,
    ) -> AnyElement {
        let ui_colors = *colors;

        let tabs = [
            (MobileTab::Feed, IconName::Menu),
            (MobileTab::Search, IconName::Search),
            (MobileTab::Settings, IconName::Settings),
        ];

        let mut nav = h_flex()
            .id("mobile-bottom-nav")
            .w_full()
            .h(px(54.))
            .flex_shrink_0()
            .pl(safe_left)
            .pr(safe_right)
            .bg(ui_colors.surface)
            .border_t_1()
            .border_color(ui_colors.border)
            .items_center();

        for (tab, icon) in tabs {
            let is_active = self.mobile_tab == tab;
            let weak = cx.weak_entity();
            let fg = if is_active {
                ui_colors.accent
            } else {
                ui_colors.muted
            };

            nav = nav.child(
                v_flex()
                    .id(SharedString::from(format!("mobile-nav-{}", tab.label())))
                    .role(Role::Tab)
                    .aria_label(tab.label())
                    .aria_selected(is_active)
                    .flex_1()
                    .h_full()
                    .items_center()
                    .justify_center()
                    .gap(px(2.))
                    .cursor_pointer()
                    .on_click(move |_, _window, cx| {
                        let _ = weak.update(cx, |app, cx| {
                            app.mobile_showing_thread = false;
                            app.mobile_tab = tab;
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .px_3()
                            .py_0p5()
                            .rounded_full()
                            .bg(if is_active {
                                ui_colors.accent_soft
                            } else {
                                transparent_black()
                            })
                            .child(Icon::new(icon).small().text_color(fg)),
                    )
                    .child(
                        div()
                            .text_size(rems(0.68))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child(tab.label()),
                    ),
            );
        }

        nav.into_any_element()
    }

    // -------------------------------------------------------------------------
    // Shared Feed List & Reader Rendering
    // -------------------------------------------------------------------------

    fn render_feed_scroll_body(
        &self,
        colors: &UiColors,
        mono_font: &str,
        is_mobile: bool,
        now_unix: i64,
        cx: &Context<Self>,
    ) -> AnyElement {
        let ui_colors = *colors;

        if self.stories.is_empty() {
            let state_el = match &self.feed_state {
                LoadState::Idle | LoadState::Loading => story_list::render_list_loading(
                    &format!("Loading {}…", self.active_feed.label()),
                    &ui_colors,
                ),
                LoadState::Error(msg) => {
                    let weak_retry = cx.weak_entity();
                    let feed = self.active_feed;
                    story_list::render_list_error(
                        msg,
                        &ui_colors,
                        Rc::new(move |_window, cx| {
                            let _ = weak_retry.update(cx, |app, cx| {
                                app.load_feed(feed, 0, false, true, cx);
                            });
                        }),
                    )
                }
                LoadState::Loaded(_) => story_list::render_list_empty(
                    "No stories found",
                    "Try refreshing or selecting another feed.",
                    &ui_colors,
                ),
            };

            let scroll_box = div()
                .id(if is_mobile {
                    "mobile-feed-scroll"
                } else {
                    "desktop-feed-scroll"
                })
                .size_full()
                .overflow_y_scroll()
                .child(state_el);

            let weak_refresh = cx.weak_entity();
            let feed = self.active_feed;
            let pull_empty = pull_to_refresh(
                if is_mobile {
                    "mobile-feed-empty-ptr"
                } else {
                    "desktop-feed-empty-ptr"
                },
                scroll_box,
                PullToRefreshTarget::AlwaysAtTop,
                &self.feed_pull_to_refresh,
                self.feed_refreshing,
                &ui_colors,
                Rc::new(move |_window, cx| {
                    let _ = weak_refresh.update(cx, |app, cx| {
                        app.load_feed(feed, 0, false, true, cx);
                    });
                }),
            );

            return div()
                .relative()
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_hidden()
                .child(pull_empty)
                .into_any_element();
        }

        let stories = Arc::clone(&self.stories);
        let story_count = stories.len();
        let selected_id = self.selected_story_id;
        let mono_family = mono_font.to_string();
        let has_more = self.has_more_feed_pages();
        let loading_more = self.loading_more_feed;

        let weak_select = cx.weak_entity();
        let on_select: super::IdCallback = Rc::new(move |story_id, _window, cx| {
            let _ = weak_select.update(cx, |app, cx| {
                app.select_story(story_id, None, is_mobile, cx);
            });
        });

        let weak_open = cx.weak_entity();
        let on_open_url: super::UrlCallback = Rc::new(move |url, _window, cx| {
            let _ = weak_open.update(cx, |app, cx| {
                app.open_external_url(url, cx);
            });
        });

        let weak_more = cx.weak_entity();
        let on_load_more: super::ActionCallback = Rc::new(move |_window, cx| {
            let _ = weak_more.update(cx, |app, cx| {
                if !app.loading_more_feed && app.has_more_feed_pages() {
                    app.load_feed(app.active_feed, app.feed_page + 1, true, false, cx);
                }
            });
        });

        let list_el = list(self.feed_list_state.clone(), move |ix, _window, _cx| {
            if ix < story_count {
                let story = &stories[ix];
                let selected = selected_id == Some(story.id);
                if is_mobile {
                    story_list::render_mobile_story_card(
                        story,
                        selected,
                        &ui_colors,
                        now_unix,
                        &mono_family,
                        on_open_url.clone(),
                        on_select.clone(),
                    )
                } else {
                    story_list::render_desktop_story_row(
                        ix + 1,
                        story,
                        selected,
                        &ui_colors,
                        now_unix,
                        &mono_family,
                        on_select.clone(),
                        on_open_url.clone(),
                    )
                }
            } else if ix == story_count {
                story_list::render_feed_footer(
                    has_more,
                    loading_more,
                    &ui_colors,
                    on_load_more.clone(),
                )
            } else {
                div().into_any_element()
            }
        })
        .size_full();

        let smooth_list = smooth_scroll(
            list_el,
            SmoothScrollTarget::List(self.feed_list_state.clone()),
            &self.feed_smooth_scroll,
            self.settings.smooth_scrolling,
        );

        let weak_refresh = cx.weak_entity();
        let feed = self.active_feed;
        let pull_list = pull_to_refresh(
            if is_mobile {
                "mobile-feed-ptr"
            } else {
                "desktop-feed-ptr"
            },
            smooth_list,
            PullToRefreshTarget::List(self.feed_list_state.clone()),
            &self.feed_pull_to_refresh,
            self.feed_refreshing,
            &ui_colors,
            Rc::new(move |_window, cx| {
                let _ = weak_refresh.update(cx, |app, cx| {
                    app.load_feed(feed, 0, false, true, cx);
                });
            }),
        );

        div()
            .id(if is_mobile {
                "mobile-feed-scroll"
            } else {
                "desktop-feed-scroll"
            })
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(pull_list)
            .vertical_scrollbar(&self.feed_list_state)
            .into_any_element()
    }

    fn render_reader(
        &self,
        colors: &UiColors,
        is_mobile: bool,
        now_unix: i64,
        cx: &Context<Self>,
    ) -> AnyElement {
        let active_story = self.active_story_arc();
        let loaded_comment_count = self.cached_loaded_comment_count;
        let comment_count = active_story
            .as_deref()
            .map(|story| story.num_comments.max(loaded_comment_count))
            .unwrap_or(loaded_comment_count);

        let toolbar = ReaderToolbarState {
            comment_count,
            loaded_comment_count,
            sort: self.settings.comment_sort,
            all_collapsed: self.all_collapsed,
            refreshing: self.thread_refreshing,
        };
        let callbacks = self.build_reader_callbacks(is_mobile, cx);

        reader_pane::render_reader_pane(
            active_story,
            &self.thread_state,
            Arc::clone(&self.cached_selftext_blocks),
            Arc::clone(&self.cached_visible_comments),
            &toolbar,
            colors,
            &self.settings,
            &self.reader_list_state,
            &self.reader_smooth_scroll,
            &self.reader_pull_to_refresh,
            is_mobile,
            now_unix,
            callbacks,
        )
    }
}

impl Focusable for XolhnApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for XolhnApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = UiColors::from(self.settings.theme);
        let now_unix = current_unix_timestamp();
        let is_compact = is_compact_layout(window);

        if self.last_is_compact != Some(is_compact) {
            self.last_is_compact = Some(is_compact);
            self.reset_reader_list_state(is_compact);
            self.feed_list_state.remeasure();
            self.search_list_state.remeasure();
        }
        let expected_reader = self.expected_reader_item_count(is_compact);
        if self.reader_list_state.item_count() != expected_reader {
            self.reader_list_state
                .reset_with_uniform_height(expected_reader, px(110.));
        }
        let expected_feed = self.expected_feed_item_count();
        if self.feed_list_state.item_count() != expected_feed {
            self.feed_list_state
                .reset_with_uniform_height(expected_feed, px(68.));
        }
        let expected_search = self.expected_search_item_count();
        if self.search_list_state.item_count() != expected_search {
            self.search_list_state
                .reset_with_uniform_height(expected_search, px(96.));
        }

        let resolved_ui_font = self
            .settings
            .ui_font_family
            .clone()
            .unwrap_or_else(|| cx.theme().font_family.to_string());

        let content = if is_compact {
            self.render_mobile_layout(&colors, now_unix, window, cx)
        } else {
            self.render_desktop_layout(&colors, now_unix, cx)
        };

        div()
            .id("xolhn-app-root")
            .size_full()
            .key_context(READER_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .bg(colors.bg)
            .text_color(colors.text)
            .font_family(resolved_ui_font)
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_next_story))
            .on_action(cx.listener(Self::on_prev_story))
            .on_action(cx.listener(Self::on_open_selected_link))
            .on_action(cx.listener(Self::on_refresh_active))
            .on_action(cx.listener(Self::on_cycle_theme))
            .on_action(cx.listener(Self::on_toggle_settings))
            .on_action(cx.listener(Self::on_feed_top))
            .on_action(cx.listener(Self::on_feed_new))
            .on_action(cx.listener(Self::on_feed_best))
            .on_action(cx.listener(Self::on_feed_ask))
            .on_action(cx.listener(Self::on_feed_show))
            .on_action(cx.listener(Self::on_feed_jobs))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            .on_action(cx.listener(Self::on_dismiss_overlay))
            .child(content)
    }
}

// -----------------------------------------------------------------------------
// Pure Helper Functions & Small Reusable Chrome Elements
// -----------------------------------------------------------------------------

fn is_compact_layout(window: &Window) -> bool {
    gpui_kit::is_mobile() || window.bounds().size.width < px(COMPACT_BREAKPOINT_PX)
}

fn feed_icon(feed: FeedKind) -> IconName {
    match feed {
        FeedKind::Top => IconName::Star,
        FeedKind::New => IconName::Calendar,
        FeedKind::Best => IconName::ThumbsUp,
        FeedKind::Ask => IconName::Info,
        FeedKind::Show => IconName::Globe,
        FeedKind::Jobs => IconName::Building2,
    }
}

fn render_brand_mark(colors: &UiColors, size_px: f32) -> AnyElement {
    div()
        .size(px(size_px))
        .rounded(px(5.))
        .bg(colors.accent)
        .text_color(colors.accent_fg)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .text_size(rems(0.76))
        .font_weight(FontWeight::EXTRA_BOLD)
        .child("X")
        .into_any_element()
}

fn render_rail_button(
    id: SharedString,
    icon: IconName,
    label: &'static str,
    active: bool,
    colors: &UiColors,
    on_click: super::ActionCallback,
) -> AnyElement {
    let ui_colors = *colors;
    let fg = if active {
        ui_colors.accent
    } else {
        ui_colors.muted
    };

    div()
        .w_full()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .when(active, |this| {
            this.child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(7.))
                    .bottom(px(7.))
                    .w(px(3.))
                    .rounded(px(2.))
                    .bg(ui_colors.accent),
            )
        })
        .child(
            v_flex()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .aria_selected(active)
                .w(px(40.))
                .h(px(38.))
                .rounded(px(7.))
                .items_center()
                .justify_center()
                .gap(px(2.))
                .cursor_pointer()
                .bg(if active {
                    ui_colors.accent_soft
                } else {
                    transparent_black()
                })
                .when(!active, |this| {
                    this.hover(move |s| s.bg(ui_colors.surface_2))
                })
                .on_click(move |_, window, cx| {
                    on_click(window, cx);
                })
                .child(Icon::new(icon).small().text_color(fg))
                .child(
                    div()
                        .text_size(px(9.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg)
                        .child(label),
                ),
        )
        .into_any_element()
}

fn render_toast_overlay(
    message: &str,
    colors: &UiColors,
    is_mobile: bool,
    mobile_showing_thread: bool,
    safe_area: crate::SafeAreaInsets,
) -> AnyElement {
    let pill = div()
        .px_3()
        .py_1p5()
        .rounded(px(8.))
        .bg(colors.surface_2)
        .border_1()
        .border_color(colors.accent)
        .shadow_md()
        .text_size(rems(0.78))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.text)
        .child(message.to_string());

    if is_mobile {
        let bottom_offset = if mobile_showing_thread {
            px(64.)
        } else {
            px(66.)
        };
        h_flex()
            .absolute()
            .left(px(12.) + px(safe_area.left))
            .right(px(12.) + px(safe_area.right))
            .bottom(bottom_offset + px(safe_area.bottom))
            .justify_center()
            .child(pill)
            .into_any_element()
    } else {
        div()
            .absolute()
            .right_3()
            .bottom_3()
            .child(pill)
            .into_any_element()
    }
}

fn append_unique_stories(target: &mut Vec<Story>, incoming: &[Story]) {
    let mut seen: HashSet<u64> = target.iter().map(|story| story.id).collect();
    for story in incoming {
        if seen.insert(story.id) {
            target.push(story.clone());
        }
    }
}

fn feed_has_more_pages(
    feed: FeedKind,
    current_page: usize,
    nb_pages: usize,
    has_stories: bool,
) -> bool {
    if !has_stories {
        return false;
    }
    // Algolia's front_page tag only returns page 0 (nb_pages == 1), while page 1+
    // transparently falls back to recent stories in the last 48 hours.
    if feed == FeedKind::Top && current_page == 0 {
        return true;
    }
    current_page + 1 < nb_pages
}

fn search_hit_story_id(hit: &SearchHit) -> u64 {
    match hit {
        SearchHit::Story(story) => story.id,
        SearchHit::Comment(comment) => comment.story_id,
    }
}

fn step_index(current: Option<usize>, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    match current {
        Some(idx) => {
            if delta >= 0 {
                idx.saturating_add(delta as usize).min(len - 1)
            } else {
                idx.saturating_sub(delta.unsigned_abs())
            }
        }
        None => {
            if delta >= 0 {
                0
            } else {
                len - 1
            }
        }
    }
}

/// Computes the child index inside `reader_list_state` for the next root (`depth == 1`) comment,
/// wrapping back to the first root comment when already at or past the last one.
fn next_root_comment_child_index(
    visible_comments: &[VisibleCommentRow],
    current_top_item: usize,
    header_child_offset: usize,
    last_jumped: Option<usize>,
    at_bottom: bool,
) -> Option<usize> {
    let anchor = match last_jumped {
        Some(prev) if at_bottom || prev.abs_diff(current_top_item) <= 1 => {
            prev.max(current_top_item)
        }
        _ => current_top_item,
    };

    let mut first_root = None;
    for (idx, row) in visible_comments.iter().enumerate() {
        if row.depth != 1 {
            continue;
        }
        let child_idx = idx + header_child_offset;
        if first_root.is_none() {
            first_root = Some(child_idx);
        }
        if child_idx > anchor {
            return Some(child_idx);
        }
    }
    first_root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_story(id: u64) -> Story {
        Story {
            id,
            title: format!("Story {id}"),
            url: None,
            domain: None,
            author: "alice".to_string(),
            points: 10,
            num_comments: 3,
            created_at_i: 1_700_000_000,
            text_html: None,
            tags: vec!["story".to_string()],
        }
    }

    fn sample_row(id: u64, depth: usize, root_index: usize) -> VisibleCommentRow {
        VisibleCommentRow {
            id,
            author: Some("bob".to_string()),
            text_html: "<p>hi</p>".to_string(),
            blocks: Arc::new(Vec::new()),
            created_at_i: 1_700_000_000,
            depth,
            is_collapsed: false,
            subtree_count: 1,
            is_op: false,
            root_index,
            has_children: false,
        }
    }

    #[test]
    fn append_unique_stories_preserves_order_and_deduplicates() {
        let mut target = vec![sample_story(1), sample_story(2)];
        append_unique_stories(
            &mut target,
            &[
                sample_story(2),
                sample_story(3),
                sample_story(1),
                sample_story(4),
            ],
        );
        let ids: Vec<u64> = target.into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn next_root_comment_uses_header_offset_and_wraps() {
        let rows = vec![
            sample_row(10, 1, 0), // child 1 on mobile
            sample_row(11, 2, 0), // child 2
            sample_row(20, 1, 1), // child 3
            sample_row(21, 2, 1), // child 4
            sample_row(30, 1, 2), // child 5
        ];

        assert_eq!(
            next_root_comment_child_index(&rows, 0, 1, None, false),
            Some(1)
        );
        assert_eq!(
            next_root_comment_child_index(&rows, 1, 1, Some(1), false),
            Some(3)
        );
        // Off-by-one top_item after jumping to child 3 still advances past child 3.
        assert_eq!(
            next_root_comment_child_index(&rows, 2, 1, Some(3), false),
            Some(5)
        );
        // Bottom clamping keeps top_item at 2 while jumping 3 -> 5 -> wraps to 1.
        assert_eq!(
            next_root_comment_child_index(&rows, 2, 1, Some(5), true),
            Some(1)
        );
        // Manual scroll back up far above last_jumped honors new scroll top.
        assert_eq!(
            next_root_comment_child_index(&rows, 0, 1, Some(5), false),
            Some(1)
        );
        assert_eq!(next_root_comment_child_index(&[], 0, 1, None, false), None);
    }

    #[test]
    fn feed_has_more_pages_handles_top_page_zero_fallback() {
        assert!(feed_has_more_pages(FeedKind::Top, 0, 1, true));
        assert!(!feed_has_more_pages(FeedKind::Top, 0, 1, false));
        assert!(!feed_has_more_pages(FeedKind::Top, 1, 2, true));
        assert!(feed_has_more_pages(FeedKind::New, 0, 5, true));
        assert!(!feed_has_more_pages(FeedKind::New, 4, 5, true));
    }

    #[test]
    fn step_index_clamps_within_bounds() {
        assert_eq!(step_index(None, 5, 1), 0);
        assert_eq!(step_index(None, 5, -1), 4);
        assert_eq!(step_index(Some(0), 5, -1), 0);
        assert_eq!(step_index(Some(4), 5, 1), 4);
        assert_eq!(step_index(Some(2), 5, 1), 3);
        assert_eq!(step_index(Some(2), 5, -1), 1);
        assert_eq!(step_index(None, 0, 1), 0);
    }
}
