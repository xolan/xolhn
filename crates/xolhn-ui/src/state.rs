use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use xolhn_core::html::{ContentBlock, parse_hn_html, plain_text_excerpt};
use xolhn_core::model::{Comment, CommentSort, SearchHit, StoryThread, format_relative_time};
use xolhn_core::theme::ThemePalette;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppPaths {
    pub config_dir: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
}

pub type ChromeCallback = Arc<dyn Fn(ThemePalette) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LoadState<T> {
    #[default]
    Idle,
    Loading,
    Loaded(T),
    Error(String),
}

impl<T> LoadState<T> {
    pub const fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    pub const fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    pub const fn data(&self) -> Option<&T> {
        if let Self::Loaded(data) = self {
            Some(data)
        } else {
            None
        }
    }

    pub fn error(&self) -> Option<&str> {
        if let Self::Error(err) = self {
            Some(err.as_str())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MobileTab {
    #[default]
    Feed,
    Search,
    Settings,
}

impl MobileTab {
    pub const ALL: [MobileTab; 3] = [Self::Feed, Self::Search, Self::Settings];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Feed => "Feed",
            Self::Search => "Search",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleCommentRow {
    pub id: u64,
    pub author: Option<String>,
    pub text_html: String,
    pub blocks: Arc<Vec<ContentBlock>>,
    pub created_at_i: i64,
    pub depth: usize,
    pub is_collapsed: bool,
    pub subtree_count: usize,
    pub is_op: bool,
    pub root_index: usize,
    pub has_children: bool,
}

impl VisibleCommentRow {
    pub fn from_comment(
        comment: &Comment,
        story_author: &str,
        depth: usize,
        root_index: usize,
        is_collapsed: bool,
    ) -> Self {
        Self::from_prepared_comment(
            comment,
            story_author,
            depth,
            root_index,
            is_collapsed,
            comment.subtree_count(),
            Arc::new(parse_hn_html(&comment.text_html)),
        )
    }

    pub fn from_prepared_comment(
        comment: &Comment,
        story_author: &str,
        depth: usize,
        root_index: usize,
        is_collapsed: bool,
        subtree_count: usize,
        blocks: Arc<Vec<ContentBlock>>,
    ) -> Self {
        Self {
            id: comment.id,
            author: comment.author.clone(),
            text_html: comment.text_html.clone(),
            blocks,
            created_at_i: comment.created_at_i,
            depth,
            is_collapsed,
            subtree_count,
            is_op: comment.is_op(story_author),
            root_index,
            has_children: !comment.children.is_empty(),
        }
    }

    pub fn relative_time(&self, now_unix: i64) -> String {
        format_relative_time(self.created_at_i, now_unix)
    }
}

/// Parses optional story self-text HTML once into shared [`ContentBlock`]s.
pub fn parse_selftext_blocks(text_html: Option<&str>) -> Arc<Vec<ContentBlock>> {
    let raw = text_html.unwrap_or("").trim();
    if raw.is_empty() {
        Arc::new(Vec::new())
    } else {
        Arc::new(parse_hn_html(raw))
    }
}

/// Thread payload together with pre-parsed HTML blocks, cached subtree counts, and initial flattened rows.
#[derive(Debug, Clone)]
pub struct PreparedThread {
    pub thread: StoryThread,
    pub selftext_blocks: Arc<Vec<ContentBlock>>,
    pub comment_blocks: HashMap<u64, Arc<Vec<ContentBlock>>>,
    pub subtree_counts: HashMap<u64, usize>,
    pub all_comment_ids: Arc<Vec<u64>>,
    pub loaded_comment_count: usize,
    pub visible_comments: Arc<Vec<VisibleCommentRow>>,
    pub sort_used: CommentSort,
    pub collapsed_used: HashSet<u64>,
}

impl PreparedThread {
    /// Pre-parses HTML, computes O(N) subtree sizes, and builds visible rows (intended for background threads).
    pub fn build(thread: StoryThread, sort: CommentSort, collapsed: &HashSet<u64>) -> Self {
        let selftext_blocks = parse_selftext_blocks(thread.story.text_html.as_deref());
        let mut comment_blocks = HashMap::new();
        let mut subtree_counts = HashMap::new();
        let mut all_comment_ids = Vec::new();
        let mut loaded_comment_count = 0usize;

        for root in &thread.comments {
            loaded_comment_count += prepare_comment_nodes(
                root,
                &mut comment_blocks,
                &mut subtree_counts,
                &mut all_comment_ids,
            );
        }

        let visible_comments = Arc::new(flatten_comments_prepared(
            &thread.comments,
            &thread.story.author,
            sort,
            collapsed,
            &comment_blocks,
            &subtree_counts,
        ));

        Self {
            thread,
            selftext_blocks,
            comment_blocks,
            subtree_counts,
            all_comment_ids: Arc::new(all_comment_ids),
            loaded_comment_count,
            visible_comments,
            sort_used: sort,
            collapsed_used: collapsed.clone(),
        }
    }
}

fn prepare_comment_nodes(
    comment: &Comment,
    comment_blocks: &mut HashMap<u64, Arc<Vec<ContentBlock>>>,
    subtree_counts: &mut HashMap<u64, usize>,
    all_comment_ids: &mut Vec<u64>,
) -> usize {
    all_comment_ids.push(comment.id);
    comment_blocks.insert(comment.id, Arc::new(parse_hn_html(&comment.text_html)));

    let mut count = 1usize;
    for child in &comment.children {
        count += prepare_comment_nodes(child, comment_blocks, subtree_counts, all_comment_ids);
    }
    subtree_counts.insert(comment.id, count);
    count
}

/// Search hit paired with an optional precomputed plain-text excerpt for comment hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSearchHit {
    pub hit: SearchHit,
    pub excerpt: Option<String>,
}

impl PreparedSearchHit {
    pub fn from_hit(hit: SearchHit) -> Self {
        let excerpt = match &hit {
            SearchHit::Comment(comment) => Some(plain_text_excerpt(&comment.text_html, 220)),
            SearchHit::Story(_) => None,
        };
        Self { hit, excerpt }
    }
}

pub fn flatten_comments(
    comments: &[Comment],
    story_author: &str,
    sort: CommentSort,
    collapsed: &HashSet<u64>,
) -> Vec<VisibleCommentRow> {
    let mut roots: Vec<&Comment> = comments.iter().collect();
    if sort == CommentSort::Newest {
        roots.sort_by_key(|lhs| std::cmp::Reverse(lhs.created_at_i));
    }

    let mut rows = Vec::new();
    for (root_index, root) in roots.into_iter().enumerate() {
        flatten_comment_node(root, story_author, 1, root_index, collapsed, &mut rows);
    }
    rows
}

fn flatten_comment_node(
    comment: &Comment,
    story_author: &str,
    depth: usize,
    root_index: usize,
    collapsed: &HashSet<u64>,
    out: &mut Vec<VisibleCommentRow>,
) {
    let is_collapsed = collapsed.contains(&comment.id);
    out.push(VisibleCommentRow::from_comment(
        comment,
        story_author,
        depth,
        root_index,
        is_collapsed,
    ));

    if !is_collapsed {
        for child in &comment.children {
            flatten_comment_node(child, story_author, depth + 1, root_index, collapsed, out);
        }
    }
}

/// Flattens comments using precomputed HTML blocks and subtree counts (zero HTML parsing on UI thread).
pub fn flatten_comments_prepared(
    comments: &[Comment],
    story_author: &str,
    sort: CommentSort,
    collapsed: &HashSet<u64>,
    comment_blocks: &HashMap<u64, Arc<Vec<ContentBlock>>>,
    subtree_counts: &HashMap<u64, usize>,
) -> Vec<VisibleCommentRow> {
    let mut roots: Vec<&Comment> = comments.iter().collect();
    if sort == CommentSort::Newest {
        roots.sort_by_key(|lhs| std::cmp::Reverse(lhs.created_at_i));
    }

    let mut rows = Vec::with_capacity(comments.len());
    for (root_index, root) in roots.into_iter().enumerate() {
        flatten_comment_node_prepared(
            root,
            story_author,
            1,
            root_index,
            collapsed,
            comment_blocks,
            subtree_counts,
            &mut rows,
        );
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn flatten_comment_node_prepared(
    comment: &Comment,
    story_author: &str,
    depth: usize,
    root_index: usize,
    collapsed: &HashSet<u64>,
    comment_blocks: &HashMap<u64, Arc<Vec<ContentBlock>>>,
    subtree_counts: &HashMap<u64, usize>,
    out: &mut Vec<VisibleCommentRow>,
) {
    let is_collapsed = collapsed.contains(&comment.id);
    let blocks = comment_blocks
        .get(&comment.id)
        .cloned()
        .unwrap_or_else(|| Arc::new(parse_hn_html(&comment.text_html)));
    let subtree_count = subtree_counts
        .get(&comment.id)
        .copied()
        .unwrap_or_else(|| comment.subtree_count());

    out.push(VisibleCommentRow::from_prepared_comment(
        comment,
        story_author,
        depth,
        root_index,
        is_collapsed,
        subtree_count,
        blocks,
    ));

    if !is_collapsed {
        for child in &comment.children {
            flatten_comment_node_prepared(
                child,
                story_author,
                depth + 1,
                root_index,
                collapsed,
                comment_blocks,
                subtree_counts,
                out,
            );
        }
    }
}

/// Computes `(old_range, new_count)` for `ListState::splice` when toggling collapse on `toggled_id`.
pub fn compute_comment_collapse_splice(
    old_rows: &[VisibleCommentRow],
    new_rows: &[VisibleCommentRow],
    toggled_id: u64,
    header_offset: usize,
) -> Option<(Range<usize>, usize)> {
    let old_pos = old_rows.iter().position(|r| r.id == toggled_id)?;
    let new_pos = new_rows.iter().position(|r| r.id == toggled_id)?;
    if old_pos != new_pos {
        return None;
    }

    let toggled_depth = old_rows[old_pos].depth;
    let old_end = old_rows[old_pos + 1..]
        .iter()
        .position(|r| r.depth <= toggled_depth)
        .map_or(old_rows.len(), |rel| old_pos + 1 + rel);
    let new_end = new_rows[new_pos + 1..]
        .iter()
        .position(|r| r.depth <= toggled_depth)
        .map_or(new_rows.len(), |rel| new_pos + 1 + rel);

    Some((
        (header_offset + old_pos)..(header_offset + old_end),
        new_end - new_pos,
    ))
}

pub fn collect_comment_ids(comments: &[Comment]) -> Vec<u64> {
    let mut ids = Vec::new();
    collect_comment_ids_into(comments, &mut ids);
    ids
}

fn collect_comment_ids_into(comments: &[Comment], out: &mut Vec<u64>) {
    for comment in comments {
        out.push(comment.id);
        collect_comment_ids_into(&comment.children, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_comment(
        id: u64,
        author: Option<&str>,
        created_at_i: i64,
        children: Vec<Comment>,
    ) -> Comment {
        Comment {
            id,
            parent_id: None,
            story_id: 100,
            author: author.map(str::to_string),
            text_html: format!("<p>comment {id}</p>"),
            created_at_i,
            children,
        }
    }

    #[test]
    fn empty_comments_produces_empty_rows_and_ids() {
        let rows = flatten_comments(&[], "alice", CommentSort::HnDefault, &HashSet::new());
        assert!(rows.is_empty());
        assert!(collect_comment_ids(&[]).is_empty());
    }

    #[test]
    fn nested_depth_tracking_and_root_index() {
        let tree = vec![
            make_comment(
                1,
                Some("alice"),
                1_000,
                vec![make_comment(
                    2,
                    Some("bob"),
                    1_010,
                    vec![make_comment(3, Some("carol"), 1_020, vec![])],
                )],
            ),
            make_comment(
                4,
                Some("dave"),
                1_030,
                vec![make_comment(5, Some("erin"), 1_040, vec![])],
            ),
        ];

        let rows = flatten_comments(&tree, "alice", CommentSort::HnDefault, &HashSet::new());
        let summary: Vec<(u64, usize, usize, usize, bool)> = rows
            .iter()
            .map(|r| (r.id, r.depth, r.root_index, r.subtree_count, r.has_children))
            .collect();

        assert_eq!(
            summary,
            vec![
                (1, 1, 0, 3, true),
                (2, 2, 0, 2, true),
                (3, 3, 0, 1, false),
                (4, 1, 1, 2, true),
                (5, 2, 1, 1, false),
            ]
        );
    }

    #[test]
    fn collapsing_node_hides_descendants_while_keeping_sibling_roots_visible() {
        let tree = vec![
            make_comment(
                10,
                Some("alice"),
                1_000,
                vec![make_comment(
                    11,
                    Some("bob"),
                    1_010,
                    vec![make_comment(12, Some("carol"), 1_020, vec![])],
                )],
            ),
            make_comment(
                20,
                Some("dave"),
                1_030,
                vec![make_comment(21, Some("erin"), 1_040, vec![])],
            ),
        ];

        let collapsed = HashSet::from([10]);
        let rows = flatten_comments(&tree, "alice", CommentSort::HnDefault, &collapsed);

        let ids: Vec<u64> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![10, 20, 21]);
        assert!(rows[0].is_collapsed);
        assert_eq!(rows[0].subtree_count, 3);
        assert!(rows[0].has_children);
        assert!(!rows[1].is_collapsed);
        assert_eq!(rows[1].root_index, 1);
    }

    #[test]
    fn newest_sort_reorders_top_level_roots_without_scrambling_children() {
        let tree = vec![
            make_comment(
                1,
                Some("alice"),
                1_000,
                vec![
                    make_comment(11, Some("bob"), 900, vec![]),
                    make_comment(12, Some("carol"), 1_500, vec![]),
                ],
            ),
            make_comment(
                2,
                Some("dave"),
                2_000,
                vec![
                    make_comment(21, Some("erin"), 2_500, vec![]),
                    make_comment(22, Some("frank"), 2_100, vec![]),
                ],
            ),
            make_comment(3, Some("grace"), 2_000, vec![]),
        ];

        let rows = flatten_comments(&tree, "alice", CommentSort::Newest, &HashSet::new());
        let ids_and_roots: Vec<(u64, usize)> = rows.iter().map(|r| (r.id, r.root_index)).collect();

        // Root 2 and Root 3 tie at 2_000 so original order (2 then 3) is preserved,
        // followed by Root 1 (1_000). Child order inside Root 2 (21, 22) and Root 1 (11, 12) is preserved.
        assert_eq!(
            ids_and_roots,
            vec![(2, 0), (21, 0), (22, 0), (3, 1), (1, 2), (11, 2), (12, 2),]
        );
    }

    #[test]
    fn is_op_matches_story_author() {
        let tree = vec![make_comment(
            1,
            Some("pg"),
            1_000,
            vec![
                make_comment(2, Some("other"), 1_010, vec![]),
                make_comment(3, None, 1_020, vec![]),
                make_comment(4, Some("pg"), 1_030, vec![]),
            ],
        )];

        let rows = flatten_comments(&tree, "pg", CommentSort::HnDefault, &HashSet::new());
        let op_flags: Vec<(u64, bool)> = rows.iter().map(|r| (r.id, r.is_op)).collect();
        assert_eq!(op_flags, vec![(1, true), (2, false), (3, false), (4, true)]);

        let empty_author_rows =
            flatten_comments(&tree, "", CommentSort::HnDefault, &HashSet::new());
        assert!(empty_author_rows.iter().all(|r| !r.is_op));
    }

    #[test]
    fn collect_comment_ids_returns_all_ids_recursively() {
        let tree = vec![
            make_comment(
                1,
                Some("a"),
                1_000,
                vec![
                    make_comment(
                        2,
                        Some("b"),
                        1_010,
                        vec![make_comment(3, Some("c"), 1_020, vec![])],
                    ),
                    make_comment(4, Some("d"), 1_030, vec![]),
                ],
            ),
            make_comment(5, Some("e"), 1_040, vec![]),
        ];

        assert_eq!(collect_comment_ids(&tree), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn load_state_helpers_work() {
        let idle: LoadState<u32> = LoadState::default();
        assert!(idle.is_idle());
        assert!(!idle.is_loading());
        assert_eq!(idle.data(), None);
        assert_eq!(idle.error(), None);

        let loading: LoadState<u32> = LoadState::Loading;
        assert!(loading.is_loading());

        let loaded = LoadState::Loaded(42);
        assert!(loaded.is_loaded());
        assert_eq!(loaded.data(), Some(&42));

        let err: LoadState<u32> = LoadState::Error("network timeout".to_string());
        assert!(err.is_error());
        assert_eq!(err.error(), Some("network timeout"));
    }

    #[test]
    fn prepared_thread_precomputes_blocks_counts_and_matches_flatten_comments() {
        use std::time::Instant;
        use xolhn_core::model::Story;

        // Build a synthetic 600-comment nested thread (60 roots x 10 nodes per subtree).
        let mut comments = Vec::new();
        let mut next_id = 1_000u64;
        for r in 0..60 {
            let root_id = next_id;
            next_id += 1;
            let mut sub_children = Vec::new();
            for _c in 0..9 {
                let cid = next_id;
                next_id += 1;
                sub_children.push(Comment {
                    id: cid,
                    parent_id: Some(root_id),
                    story_id: 1,
                    author: Some(if cid % 5 == 0 { "pg" } else { "alice" }.to_string()),
                    text_html: format!(
                        "<p>Comment <b>{cid}</b> with <a href=\"https://example.com/{cid}\">link</a> &amp; <code>code()</code>.</p>"
                    ),
                    created_at_i: 1_700_000_000 + (cid as i64),
                    children: Vec::new(),
                });
            }
            // Add deeper nesting on first child so depth >= 3 exists.
            let nested = sub_children.split_off(7);
            if let Some(first_child) = sub_children.first_mut() {
                first_child.children = nested;
            }
            comments.push(Comment {
                id: root_id,
                parent_id: Some(1),
                story_id: 1,
                author: Some("pg".to_string()),
                text_html: format!(
                    "<p>Root thread {r} opening statement.</p><p>> Quoted point</p>"
                ),
                created_at_i: 1_700_000_000 + (r as i64) * 10,
                children: sub_children,
            });
        }

        let thread = StoryThread {
            story: Story {
                id: 1,
                title: "Ask HN: Benchmarks".to_string(),
                url: None,
                domain: None,
                author: "pg".to_string(),
                points: 300,
                num_comments: 600,
                created_at_i: 1_700_000_000,
                text_html: Some("<p>Body text</p>".to_string()),
                tags: vec!["ask_hn".to_string()],
            },
            comments,
            fetched_at: 1_700_001_000,
        };

        let start_build = Instant::now();
        let collapsed = HashSet::from([1_000u64]);
        let prepared = PreparedThread::build(thread.clone(), CommentSort::Newest, &collapsed);
        let build_elapsed = start_build.elapsed();

        assert_eq!(prepared.loaded_comment_count, 600);
        assert_eq!(prepared.all_comment_ids.len(), 600);
        assert_eq!(prepared.comment_blocks.len(), 600);
        assert_eq!(prepared.subtree_counts.len(), 600);
        assert_eq!(prepared.selftext_blocks.len(), 1);
        // Root 1_000 (with 9 descendants) is collapsed -> 591 visible rows.
        assert_eq!(prepared.visible_comments.len(), 591);

        let expected = flatten_comments(&thread.comments, "pg", CommentSort::Newest, &collapsed);
        assert_eq!(prepared.visible_comments.as_slice(), expected.as_slice());

        let start_reflat = Instant::now();
        let reflat = flatten_comments_prepared(
            &prepared.thread.comments,
            &prepared.thread.story.author,
            CommentSort::Newest,
            &HashSet::new(),
            &prepared.comment_blocks,
            &prepared.subtree_counts,
        );
        let reflat_elapsed = start_reflat.elapsed();
        assert_eq!(reflat.len(), 600);
        println!(
            "PreparedThread::build(600)={:?}, flatten_comments_prepared(600)={:?}",
            build_elapsed, reflat_elapsed
        );
    }

    #[test]
    fn compute_comment_collapse_splice_handles_collapse_expand_nested_and_leaf() {
        let tree = vec![
            make_comment(
                10,
                Some("alice"),
                1_000,
                vec![
                    make_comment(
                        11,
                        Some("bob"),
                        1_010,
                        vec![make_comment(12, Some("carol"), 1_020, vec![])],
                    ),
                    make_comment(13, Some("dave"), 1_025, vec![]),
                ],
            ),
            make_comment(20, Some("erin"), 1_030, vec![]),
        ];

        let expanded = flatten_comments(&tree, "alice", CommentSort::HnDefault, &HashSet::new());
        let collapsed_10 =
            flatten_comments(&tree, "alice", CommentSort::HnDefault, &HashSet::from([10]));
        // Collapsing root 10: rows 10,11,12,13 (indices 0..4 in comments, 1..5 in list) -> 1 row (10)
        assert_eq!(
            compute_comment_collapse_splice(&expanded, &collapsed_10, 10, 1),
            Some((1..5, 1))
        );
        // Expanding root 10 back: 1 row (index 1..2 in list) -> 4 rows
        assert_eq!(
            compute_comment_collapse_splice(&collapsed_10, &expanded, 10, 1),
            Some((1..2, 4))
        );

        // Collapsing nested comment 11 (rows at comment idx 1..3 -> list 2..4) -> 1 row
        let collapsed_11 =
            flatten_comments(&tree, "alice", CommentSort::HnDefault, &HashSet::from([11]));
        assert_eq!(
            compute_comment_collapse_splice(&expanded, &collapsed_11, 11, 1),
            Some((2..4, 1))
        );

        // Collapsing leaf comment 20 (comment idx 4 -> list 5..6) -> 1 row
        let collapsed_20 =
            flatten_comments(&tree, "alice", CommentSort::HnDefault, &HashSet::from([20]));
        assert_eq!(
            compute_comment_collapse_splice(&expanded, &collapsed_20, 20, 1),
            Some((5..6, 1))
        );

        // Missing ID returns None
        assert_eq!(
            compute_comment_collapse_splice(&expanded, &collapsed_10, 999, 1),
            None
        );
    }

    #[test]
    fn prepared_search_hit_precomputes_comment_excerpt() {
        use xolhn_core::model::{SearchCommentHit, Story};

        let story_hit = SearchHit::Story(Story {
            id: 1,
            title: "Hello".to_string(),
            url: None,
            domain: None,
            author: "alice".to_string(),
            points: 10,
            num_comments: 2,
            created_at_i: 1_700_000_000,
            text_html: None,
            tags: vec!["story".to_string()],
        });
        assert_eq!(PreparedSearchHit::from_hit(story_hit).excerpt, None);

        let comment_hit = SearchHit::Comment(SearchCommentHit {
            id: 2,
            story_id: 1,
            story_title: "Hello".to_string(),
            story_url: None,
            author: "bob".to_string(),
            text_html: "<p>Hello &amp; <i>world</i></p>".to_string(),
            created_at_i: 1_700_000_100,
        });
        let prepared = PreparedSearchHit::from_hit(comment_hit);
        assert_eq!(prepared.excerpt.as_deref(), Some("Hello & world"));
    }
}
