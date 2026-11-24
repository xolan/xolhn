use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    Top,
    New,
    Best,
    Ask,
    Show,
    Jobs,
}

impl FeedKind {
    pub const ALL: [FeedKind; 6] = [
        FeedKind::Top,
        FeedKind::New,
        FeedKind::Best,
        FeedKind::Ask,
        FeedKind::Show,
        FeedKind::Jobs,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            FeedKind::Top => "Top Stories",
            FeedKind::New => "New Stories",
            FeedKind::Best => "Best Stories",
            FeedKind::Ask => "Ask HN",
            FeedKind::Show => "Show HN",
            FeedKind::Jobs => "Jobs",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            FeedKind::Top => "Top",
            FeedKind::New => "New",
            FeedKind::Best => "Best",
            FeedKind::Ask => "Ask",
            FeedKind::Show => "Show",
            FeedKind::Jobs => "Jobs",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            FeedKind::Top => "top",
            FeedKind::New => "new",
            FeedKind::Best => "best",
            FeedKind::Ask => "ask",
            FeedKind::Show => "show",
            FeedKind::Jobs => "jobs",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Story {
    pub id: u64,
    pub title: String,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub author: String,
    pub points: i64,
    pub num_comments: usize,
    pub created_at_i: i64,
    pub text_html: Option<String>,
    pub tags: Vec<String>,
}

impl Story {
    /// Returns the story's external URL, or its Hacker News item URL for self-posts.
    pub fn effective_url(&self) -> String {
        match &self.url {
            Some(url) if !url.trim().is_empty() => url.clone(),
            _ => format!("https://news.ycombinator.com/item?id={}", self.id),
        }
    }

    /// Returns a concise host domain for display.
    pub fn display_domain(&self) -> String {
        if let Some(domain) = &self.domain
            && !domain.is_empty()
        {
            return domain.clone();
        }
        if let Some(url) = &self.url
            && let Some(host) = Self::extract_domain(url)
        {
            return host;
        }
        "news.ycombinator.com".to_string()
    }

    /// Formats a relative age string against `now_unix` (e.g. `3h ago`, `just now`).
    pub fn relative_time(&self, now_unix: i64) -> String {
        format_relative_time(self.created_at_i, now_unix)
    }

    pub fn extract_domain(raw_url: &str) -> Option<String> {
        let parsed = Url::parse(raw_url).ok()?;
        let host = parsed.host_str()?;
        let trimmed = host.strip_prefix("www.").unwrap_or(host);
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub story_id: u64,
    pub author: Option<String>,
    pub text_html: String,
    pub created_at_i: i64,
    pub children: Vec<Comment>,
}

impl Comment {
    /// Returns the total number of comments in this subtree, including `self`.
    pub fn subtree_count(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(Comment::subtree_count)
            .sum::<usize>()
    }

    pub fn is_op(&self, story_author: &str) -> bool {
        !story_author.is_empty()
            && self
                .author
                .as_deref()
                .is_some_and(|author| author == story_author)
    }

    pub fn relative_time(&self, now_unix: i64) -> String {
        format_relative_time(self.created_at_i, now_unix)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryThread {
    pub story: Story,
    pub comments: Vec<Comment>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedPage {
    pub feed: FeedKind,
    pub page: usize,
    pub nb_pages: usize,
    pub nb_hits: usize,
    pub stories: Vec<Story>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchFilter {
    #[default]
    All,
    Stories,
    Comments,
}

impl SearchFilter {
    pub const ALL: [SearchFilter; 3] = [
        SearchFilter::All,
        SearchFilter::Stories,
        SearchFilter::Comments,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            SearchFilter::All => "All",
            SearchFilter::Stories => "Stories",
            SearchFilter::Comments => "Comments",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSort {
    #[default]
    Relevance,
    Newest,
}

impl SearchSort {
    pub const fn label(self) -> &'static str {
        match self {
            SearchSort::Relevance => "Relevance",
            SearchSort::Newest => "Newest",
        }
    }

    pub const fn toggle(self) -> Self {
        match self {
            SearchSort::Relevance => SearchSort::Newest,
            SearchSort::Newest => SearchSort::Relevance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentSort {
    #[default]
    HnDefault,
    Newest,
}

impl CommentSort {
    pub const fn label(self) -> &'static str {
        match self {
            CommentSort::HnDefault => "HN Default",
            CommentSort::Newest => "Newest",
        }
    }

    pub const fn toggle(self) -> Self {
        match self {
            CommentSort::HnDefault => CommentSort::Newest,
            CommentSort::Newest => CommentSort::HnDefault,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchCommentHit {
    pub id: u64,
    pub story_id: u64,
    pub story_title: String,
    pub story_url: Option<String>,
    pub author: String,
    pub text_html: String,
    pub created_at_i: i64,
}

impl SearchCommentHit {
    pub fn relative_time(&self, now_unix: i64) -> String {
        format_relative_time(self.created_at_i, now_unix)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchHit {
    Story(Story),
    Comment(SearchCommentHit),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResults {
    pub query: String,
    pub filter: SearchFilter,
    pub sort: SearchSort,
    pub page: usize,
    pub nb_pages: usize,
    pub nb_hits: usize,
    pub hits: Vec<SearchHit>,
}

pub fn format_relative_time(created_at_i: i64, now_unix: i64) -> String {
    let delta = now_unix.saturating_sub(created_at_i);
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3600)
    } else if delta < 86_400 * 30 {
        format!("{}d ago", delta / 86_400)
    } else if delta < 86_400 * 365 {
        format!("{}mo ago", delta / (86_400 * 30))
    } else {
        format!("{}y ago", delta / (86_400 * 365))
    }
}

pub fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
