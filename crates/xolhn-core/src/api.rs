use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::model::{
    Comment, FeedKind, FeedPage, SearchCommentHit, SearchFilter, SearchHit, SearchResults,
    SearchSort, Story, StoryThread, current_unix_timestamp,
};

const HITS_PER_PAGE: &str = "30";
const TOP_FALLBACK_WINDOW_SECS: i64 = 172_800; // 48 hours
const BEST_WINDOW_SECS: i64 = 604_800; // 7 days

#[derive(Debug, Clone, Error)]
pub enum HnError {
    #[error("network error: {0}")]
    Http(String),
    #[error("rate limited by Algolia HN API (HTTP 429)")]
    RateLimited,
    #[error("item {0} not found")]
    NotFound(u64),
    #[error("failed to decode API response: {0}")]
    Decode(String),
    #[error("cache error: {0}")]
    Cache(String),
}

pub trait HnApi: Send + Sync {
    fn fetch_feed(&self, feed: FeedKind, page: usize) -> Result<FeedPage, HnError>;
    fn fetch_thread(&self, story_id: u64) -> Result<StoryThread, HnError>;
    fn search(
        &self,
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
    ) -> Result<SearchResults, HnError>;
}

#[derive(Debug, Clone)]
pub struct AlgoliaClient {
    base_url: String,
    agent: ureq::Agent,
}

impl AlgoliaClient {
    pub const DEFAULT_BASE_URL: &str = "https://hn.algolia.com/api/v1";

    pub fn new() -> Self {
        Self::with_base_url(Self::DEFAULT_BASE_URL)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .user_agent(concat!("XolHN/", env!("CARGO_PKG_VERSION")))
            .build();

        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            agent: config.into(),
        }
    }

    /// Builds the Algolia endpoint URL for a feed page.
    pub fn build_feed_url(&self, feed: FeedKind, page: usize, now_unix: i64) -> String {
        self.try_build_feed_url(feed, page, now_unix)
            .expect("base_url must be a valid URL")
    }

    /// Builds the Algolia endpoint URL for a story/item thread.
    pub fn build_item_url(&self, story_id: u64) -> String {
        format!("{}/items/{story_id}", self.base_url)
    }

    /// Builds the Algolia endpoint URL for a search query.
    pub fn build_search_url(
        &self,
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
    ) -> String {
        self.try_build_search_url(query, filter, sort, page)
            .expect("base_url must be a valid URL")
    }

    fn try_build_feed_url(
        &self,
        feed: FeedKind,
        page: usize,
        now_unix: i64,
    ) -> Result<String, HnError> {
        let page_str = page.to_string();

        match feed {
            FeedKind::Top if page == 0 => {
                let endpoint = format!("{}/search", self.base_url);
                build_url(
                    &endpoint,
                    &[("tags", "front_page"), ("hitsPerPage", HITS_PER_PAGE)],
                )
            }
            FeedKind::Top => {
                let endpoint = format!("{}/search", self.base_url);
                let cutoff = now_unix.saturating_sub(TOP_FALLBACK_WINDOW_SECS);
                let numeric_filter = format!("created_at_i>{cutoff}");
                build_url(
                    &endpoint,
                    &[
                        ("tags", "story"),
                        ("numericFilters", &numeric_filter),
                        ("hitsPerPage", HITS_PER_PAGE),
                        ("page", &page_str),
                    ],
                )
            }
            FeedKind::New => {
                let endpoint = format!("{}/search_by_date", self.base_url);
                build_url(
                    &endpoint,
                    &[
                        ("tags", "story"),
                        ("hitsPerPage", HITS_PER_PAGE),
                        ("page", &page_str),
                    ],
                )
            }
            FeedKind::Best => {
                let endpoint = format!("{}/search", self.base_url);
                let cutoff = now_unix.saturating_sub(BEST_WINDOW_SECS);
                let numeric_filter = format!("created_at_i>{cutoff}");
                build_url(
                    &endpoint,
                    &[
                        ("tags", "story"),
                        ("numericFilters", &numeric_filter),
                        ("hitsPerPage", HITS_PER_PAGE),
                        ("page", &page_str),
                    ],
                )
            }
            FeedKind::Ask => {
                let endpoint = format!("{}/search", self.base_url);
                build_url(
                    &endpoint,
                    &[
                        ("tags", "ask_hn"),
                        ("hitsPerPage", HITS_PER_PAGE),
                        ("page", &page_str),
                    ],
                )
            }
            FeedKind::Show => {
                let endpoint = format!("{}/search", self.base_url);
                build_url(
                    &endpoint,
                    &[
                        ("tags", "show_hn"),
                        ("hitsPerPage", HITS_PER_PAGE),
                        ("page", &page_str),
                    ],
                )
            }
            FeedKind::Jobs => {
                let endpoint = format!("{}/search_by_date", self.base_url);
                build_url(
                    &endpoint,
                    &[
                        ("tags", "job"),
                        ("hitsPerPage", HITS_PER_PAGE),
                        ("page", &page_str),
                    ],
                )
            }
        }
    }

    fn try_build_search_url(
        &self,
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
    ) -> Result<String, HnError> {
        let path = match sort {
            SearchSort::Relevance => "search",
            SearchSort::Newest => "search_by_date",
        };
        let tags = match filter {
            SearchFilter::All => "(story,comment)",
            SearchFilter::Stories => "story",
            SearchFilter::Comments => "comment",
        };
        let endpoint = format!("{}/{path}", self.base_url);
        let page_str = page.to_string();

        build_url(
            &endpoint,
            &[
                ("query", query),
                ("tags", tags),
                ("hitsPerPage", HITS_PER_PAGE),
                ("page", &page_str),
            ],
        )
    }

    fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        item_id: Option<u64>,
    ) -> Result<T, HnError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|err| map_ureq_error(err, item_id))?;

        response
            .into_body()
            .read_json::<T>()
            .map_err(|err| map_ureq_error(err, item_id))
    }
}

impl Default for AlgoliaClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HnApi for AlgoliaClient {
    fn fetch_feed(&self, feed: FeedKind, page: usize) -> Result<FeedPage, HnError> {
        let now = current_unix_timestamp();
        let url = self.try_build_feed_url(feed, page, now)?;
        let response: AlgoliaSearchResponse = self.get_json(&url, None)?;
        parse_feed_response(response, feed, page, now)
    }

    fn fetch_thread(&self, story_id: u64) -> Result<StoryThread, HnError> {
        let url = self.build_item_url(story_id);
        let item: Option<AlgoliaItem> = self.get_json(&url, Some(story_id))?;
        let item = item.ok_or(HnError::NotFound(story_id))?;
        parse_item_thread(item, current_unix_timestamp())
    }

    fn search(
        &self,
        query: &str,
        filter: SearchFilter,
        sort: SearchSort,
        page: usize,
    ) -> Result<SearchResults, HnError> {
        let url = self.try_build_search_url(query, filter, sort, page)?;
        let response: AlgoliaSearchResponse = self.get_json(&url, None)?;
        parse_search_response(response, query, filter, sort, page)
    }
}

fn build_url(endpoint: &str, params: &[(&str, &str)]) -> Result<String, HnError> {
    Url::parse_with_params(endpoint, params)
        .map(|url| url.to_string())
        .map_err(|err| HnError::Http(format!("invalid request URL {endpoint:?}: {err}")))
}

fn map_ureq_error(err: ureq::Error, item_id: Option<u64>) -> HnError {
    match err {
        ureq::Error::StatusCode(429) => HnError::RateLimited,
        ureq::Error::StatusCode(404) => match item_id {
            Some(id) => HnError::NotFound(id),
            None => HnError::Http(err.to_string()),
        },
        ureq::Error::Json(json_err) => HnError::Decode(json_err.to_string()),
        other => HnError::Http(other.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct AlgoliaSearchResponse {
    #[serde(default)]
    hits: Vec<AlgoliaHit>,
    #[serde(rename = "nbHits")]
    nb_hits: Option<usize>,
    page: Option<usize>,
    #[serde(rename = "nbPages")]
    nb_pages: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AlgoliaHit {
    #[serde(rename = "objectID")]
    object_id: String,
    title: Option<String>,
    url: Option<String>,
    author: Option<String>,
    points: Option<i64>,
    num_comments: Option<usize>,
    created_at_i: Option<i64>,
    story_text: Option<String>,
    comment_text: Option<String>,
    story_id: Option<u64>,
    story_title: Option<String>,
    story_url: Option<String>,
    #[serde(rename = "_tags", default)]
    tags: Vec<String>,
}

impl AlgoliaHit {
    fn parse_id(&self) -> Result<u64, HnError> {
        self.object_id
            .parse::<u64>()
            .map_err(|err| HnError::Decode(format!("invalid objectID {:?}: {err}", self.object_id)))
    }

    fn is_comment(&self) -> bool {
        self.tags.iter().any(|tag| tag == "comment") || self.comment_text.is_some()
    }

    fn into_story(self) -> Result<Story, HnError> {
        let id = self.parse_id()?;
        let url = normalize_non_empty(self.url);
        let domain = url.as_deref().and_then(Story::extract_domain);

        Ok(Story {
            id,
            title: self.title.unwrap_or_default(),
            url,
            domain,
            author: self.author.unwrap_or_default(),
            points: self.points.unwrap_or(0),
            num_comments: self.num_comments.unwrap_or(0),
            created_at_i: self.created_at_i.unwrap_or(0),
            text_html: normalize_non_empty(self.story_text),
            tags: self.tags,
        })
    }

    fn into_search_hit(self) -> Result<SearchHit, HnError> {
        if self.is_comment() {
            let id = self.parse_id()?;
            let story_id = self.story_id.unwrap_or(id);
            let story_title = self.story_title.or(self.title).unwrap_or_default();
            let story_url = normalize_non_empty(self.story_url);
            let text_html = self.comment_text.or(self.story_text).unwrap_or_default();

            Ok(SearchHit::Comment(SearchCommentHit {
                id,
                story_id,
                story_title,
                story_url,
                author: self.author.unwrap_or_default(),
                text_html,
                created_at_i: self.created_at_i.unwrap_or(0),
            }))
        } else {
            self.into_story().map(SearchHit::Story)
        }
    }
}

#[derive(Debug, Deserialize)]
struct AlgoliaItem {
    id: u64,
    created_at_i: Option<i64>,
    #[serde(rename = "type")]
    item_type: Option<String>,
    author: Option<String>,
    title: Option<String>,
    url: Option<String>,
    text: Option<String>,
    points: Option<i64>,
    num_comments: Option<usize>,
    parent_id: Option<u64>,
    story_id: Option<u64>,
    #[serde(default)]
    children: Vec<AlgoliaItem>,
}

fn normalize_non_empty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

fn parse_feed_response(
    response: AlgoliaSearchResponse,
    feed: FeedKind,
    requested_page: usize,
    fetched_at: i64,
) -> Result<FeedPage, HnError> {
    let mut stories = Vec::with_capacity(response.hits.len());
    for hit in response.hits {
        if hit.is_comment() {
            continue;
        }
        stories.push(hit.into_story()?);
    }

    let nb_hits = response.nb_hits.unwrap_or(stories.len());
    let nb_pages = response.nb_pages.unwrap_or(0);
    let page = response.page.unwrap_or(requested_page);

    Ok(FeedPage {
        feed,
        page,
        nb_pages,
        nb_hits,
        stories,
        fetched_at,
    })
}

fn parse_search_response(
    response: AlgoliaSearchResponse,
    query: &str,
    filter: SearchFilter,
    sort: SearchSort,
    requested_page: usize,
) -> Result<SearchResults, HnError> {
    let mut hits = Vec::with_capacity(response.hits.len());
    for hit in response.hits {
        hits.push(hit.into_search_hit()?);
    }

    let nb_hits = response.nb_hits.unwrap_or(hits.len());
    let nb_pages = response.nb_pages.unwrap_or(0);
    let page = response.page.unwrap_or(requested_page);

    Ok(SearchResults {
        query: query.to_string(),
        filter,
        sort,
        page,
        nb_pages,
        nb_hits,
        hits,
    })
}

fn parse_item_thread(item: AlgoliaItem, fetched_at: i64) -> Result<StoryThread, HnError> {
    let root_story_id = item.story_id.unwrap_or(item.id);
    let comments: Vec<Comment> = item
        .children
        .into_iter()
        .filter_map(|child| parse_comment_node(child, root_story_id))
        .collect();

    let counted_comments: usize = comments.iter().map(Comment::subtree_count).sum();
    let url = normalize_non_empty(item.url);
    let domain = url.as_deref().and_then(Story::extract_domain);
    let tags = item.item_type.into_iter().collect();

    let story = Story {
        id: item.id,
        title: item.title.unwrap_or_default(),
        url,
        domain,
        author: item.author.unwrap_or_default(),
        points: item.points.unwrap_or(0),
        num_comments: item.num_comments.unwrap_or(counted_comments),
        created_at_i: item.created_at_i.unwrap_or(0),
        text_html: normalize_non_empty(item.text),
        tags,
    };

    Ok(StoryThread {
        story,
        comments,
        fetched_at,
    })
}

fn parse_comment_node(item: AlgoliaItem, root_story_id: u64) -> Option<Comment> {
    let is_comment = item.item_type.as_deref() == Some("comment") || item.parent_id.is_some();
    if !is_comment {
        return None;
    }

    let children: Vec<Comment> = item
        .children
        .into_iter()
        .filter_map(|child| parse_comment_node(child, root_story_id))
        .collect();

    let text_html = match normalize_non_empty(item.text) {
        Some(text) => text,
        None if children.is_empty() => return None,
        None => "[deleted]".to_string(),
    };

    let author = normalize_non_empty(item.author);

    Some(Comment {
        id: item.id,
        parent_id: item.parent_id,
        story_id: item.story_id.unwrap_or(root_story_id),
        author,
        text_html,
        created_at_i: item.created_at_i.unwrap_or(0),
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_map(raw_url: &str) -> Vec<(String, String)> {
        let parsed = Url::parse(raw_url).expect("generated URL should parse");
        parsed
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[test]
    fn build_feed_urls_for_all_feeds() {
        let client = AlgoliaClient::with_base_url("https://hn.algolia.com/api/v1/");
        let now = 1_700_000_000_i64;

        let top_p0 = client.build_feed_url(FeedKind::Top, 0, now);
        assert!(top_p0.starts_with("https://hn.algolia.com/api/v1/search?"));
        assert_eq!(
            query_map(&top_p0),
            vec![
                ("tags".into(), "front_page".into()),
                ("hitsPerPage".into(), "30".into()),
            ]
        );

        let top_p1 = client.build_feed_url(FeedKind::Top, 1, now);
        assert!(top_p1.starts_with("https://hn.algolia.com/api/v1/search?"));
        assert_eq!(
            query_map(&top_p1),
            vec![
                ("tags".into(), "story".into()),
                (
                    "numericFilters".into(),
                    format!("created_at_i>{}", now - TOP_FALLBACK_WINDOW_SECS)
                ),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "1".into()),
            ]
        );

        let new_url = client.build_feed_url(FeedKind::New, 2, now);
        assert!(new_url.starts_with("https://hn.algolia.com/api/v1/search_by_date?"));
        assert_eq!(
            query_map(&new_url),
            vec![
                ("tags".into(), "story".into()),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "2".into()),
            ]
        );

        let best_url = client.build_feed_url(FeedKind::Best, 0, now);
        assert!(best_url.starts_with("https://hn.algolia.com/api/v1/search?"));
        assert_eq!(
            query_map(&best_url),
            vec![
                ("tags".into(), "story".into()),
                (
                    "numericFilters".into(),
                    format!("created_at_i>{}", now - BEST_WINDOW_SECS)
                ),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "0".into()),
            ]
        );

        let ask_url = client.build_feed_url(FeedKind::Ask, 3, now);
        assert!(ask_url.starts_with("https://hn.algolia.com/api/v1/search?"));
        assert_eq!(
            query_map(&ask_url),
            vec![
                ("tags".into(), "ask_hn".into()),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "3".into()),
            ]
        );

        let show_url = client.build_feed_url(FeedKind::Show, 1, now);
        assert!(show_url.starts_with("https://hn.algolia.com/api/v1/search?"));
        assert_eq!(
            query_map(&show_url),
            vec![
                ("tags".into(), "show_hn".into()),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "1".into()),
            ]
        );

        let jobs_url = client.build_feed_url(FeedKind::Jobs, 0, now);
        assert!(jobs_url.starts_with("https://hn.algolia.com/api/v1/search_by_date?"));
        assert_eq!(
            query_map(&jobs_url),
            vec![
                ("tags".into(), "job".into()),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "0".into()),
            ]
        );

        assert_eq!(
            client.build_item_url(50_611_588),
            "https://hn.algolia.com/api/v1/items/50611588"
        );
    }

    #[test]
    fn build_search_urls_with_filters_sorts_and_encoding() {
        let client = AlgoliaClient::new();

        let all_rel = client.build_search_url(
            "rust & gpui = fun?",
            SearchFilter::All,
            SearchSort::Relevance,
            0,
        );
        assert!(all_rel.starts_with("https://hn.algolia.com/api/v1/search?"));
        assert!(!all_rel.contains("rust & gpui"));
        assert_eq!(
            query_map(&all_rel),
            vec![
                ("query".into(), "rust & gpui = fun?".into()),
                ("tags".into(), "(story,comment)".into()),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "0".into()),
            ]
        );

        let stories_newest =
            client.build_search_url("async/await", SearchFilter::Stories, SearchSort::Newest, 2);
        assert!(stories_newest.starts_with("https://hn.algolia.com/api/v1/search_by_date?"));
        assert_eq!(
            query_map(&stories_newest),
            vec![
                ("query".into(), "async/await".into()),
                ("tags".into(), "story".into()),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "2".into()),
            ]
        );

        let comments_rel = client.build_search_url(
            "sqlite \"wal\"",
            SearchFilter::Comments,
            SearchSort::Relevance,
            4,
        );
        assert!(comments_rel.starts_with("https://hn.algolia.com/api/v1/search?"));
        assert_eq!(
            query_map(&comments_rel),
            vec![
                ("query".into(), "sqlite \"wal\"".into()),
                ("tags".into(), "comment".into()),
                ("hitsPerPage".into(), "30".into()),
                ("page".into(), "4".into()),
            ]
        );
    }

    #[test]
    fn parse_front_page_job_and_ask_hn_fixtures() {
        let payload = r#"{
            "hits": [
                {
                    "objectID": "50611588",
                    "title": "Show HN: XolHN Desktop Client",
                    "url": "https://www.example.org/xolhn",
                    "author": "xolan",
                    "points": 245,
                    "num_comments": 42,
                    "created_at_i": 1700000100,
                    "story_text": null,
                    "_tags": ["story", "author_xolan", "story_50611588", "show_hn", "front_page"]
                },
                {
                    "objectID": "50611900",
                    "title": "Acme (YC S24) Is Hiring Rust Engineers",
                    "url": "https://jobs.acme.dev/rust",
                    "author": "founder",
                    "points": null,
                    "created_at_i": 1700000200,
                    "_tags": ["job", "author_founder", "story_50611900"]
                },
                {
                    "objectID": "50612000",
                    "title": "Ask HN: Best terminal fonts?",
                    "url": "",
                    "author": "typophile",
                    "points": 88,
                    "num_comments": 61,
                    "created_at_i": 1700000300,
                    "story_text": "<p>Looking for monospace recommendations.</p>",
                    "_tags": ["story", "ask_hn", "story_50612000"]
                }
            ],
            "nbHits": 3,
            "page": 0,
            "nbPages": 1
        }"#;

        let parsed: AlgoliaSearchResponse = serde_json::from_str(payload).unwrap();
        let feed = parse_feed_response(parsed, FeedKind::Top, 0, 1_700_001_000).unwrap();

        assert_eq!(feed.feed, FeedKind::Top);
        assert_eq!(feed.page, 0);
        assert_eq!(feed.nb_pages, 1);
        assert_eq!(feed.nb_hits, 3);
        assert_eq!(feed.stories.len(), 3);

        let first = &feed.stories[0];
        assert_eq!(first.id, 50_611_588);
        assert_eq!(first.domain.as_deref(), Some("example.org"));
        assert_eq!(first.points, 245);
        assert_eq!(first.num_comments, 42);
        assert_eq!(first.text_html, None);

        let job = &feed.stories[1];
        assert_eq!(job.id, 50_611_900);
        assert_eq!(job.points, 0);
        assert_eq!(job.num_comments, 0);
        assert_eq!(job.domain.as_deref(), Some("jobs.acme.dev"));

        let ask = &feed.stories[2];
        assert_eq!(ask.id, 50_612_000);
        assert_eq!(ask.url, None);
        assert_eq!(ask.domain, None);
        assert_eq!(
            ask.text_html.as_deref(),
            Some("<p>Looking for monospace recommendations.</p>")
        );
    }

    #[test]
    fn parse_item_tree_handles_deleted_nodes_and_counts_comments() {
        let payload = r#"{
            "id": 42001,
            "created_at_i": 1700000000,
            "type": "story",
            "author": "alice",
            "title": "Nested Thread Test",
            "url": "https://news.example.com/post",
            "text": null,
            "points": 120,
            "parent_id": null,
            "story_id": null,
            "children": [
                {
                    "id": 42002,
                    "created_at_i": 1700000050,
                    "type": "comment",
                    "author": null,
                    "text": null,
                    "parent_id": 42001,
                    "story_id": 42001,
                    "children": [
                        {
                            "id": 42003,
                            "created_at_i": 1700000090,
                            "type": "comment",
                            "author": "bob",
                            "text": "<p>Surviving child reply</p>",
                            "parent_id": 42002,
                            "story_id": null,
                            "children": []
                        }
                    ]
                },
                {
                    "id": 42004,
                    "created_at_i": 1700000060,
                    "type": "comment",
                    "author": null,
                    "text": "",
                    "parent_id": 42001,
                    "story_id": 42001,
                    "children": []
                },
                {
                    "id": 42005,
                    "created_at_i": 1700000070,
                    "type": "comment",
                    "author": "carol",
                    "text": "<p>Top-level visible comment</p>",
                    "parent_id": 42001,
                    "story_id": 42001,
                    "children": [
                        {
                            "id": 42006,
                            "created_at_i": 1700000080,
                            "type": "comment",
                            "author": null,
                            "text": null,
                            "parent_id": 42005,
                            "story_id": 42001,
                            "children": []
                        }
                    ]
                }
            ]
        }"#;

        let item: AlgoliaItem = serde_json::from_str(payload).unwrap();
        let thread = parse_item_thread(item, 1_700_000_500).unwrap();

        assert_eq!(thread.story.id, 42_001);
        assert_eq!(thread.story.domain.as_deref(), Some("news.example.com"));
        // 42002 (deleted parent kept) + 42003 (child) + 42005 (visible) = 3
        // 42004 and 42006 are deleted leaves and must be omitted.
        assert_eq!(thread.story.num_comments, 3);
        assert_eq!(thread.comments.len(), 2);

        let deleted_parent = &thread.comments[0];
        assert_eq!(deleted_parent.id, 42_002);
        assert_eq!(deleted_parent.author, None);
        assert_eq!(deleted_parent.text_html, "[deleted]");
        assert_eq!(deleted_parent.children.len(), 1);
        assert_eq!(deleted_parent.children[0].id, 42_003);
        assert_eq!(deleted_parent.children[0].story_id, 42_001);
        assert_eq!(deleted_parent.children[0].author.as_deref(), Some("bob"));

        let visible_top = &thread.comments[1];
        assert_eq!(visible_top.id, 42_005);
        assert!(visible_top.children.is_empty());
    }

    #[test]
    fn parse_mixed_search_results_story_and_comment() {
        let payload = r#"{
            "hits": [
                {
                    "objectID": "1001",
                    "title": "Introducing GPUI",
                    "url": "https://zed.dev/blog/gpui",
                    "author": "nathan",
                    "points": 512,
                    "num_comments": 140,
                    "created_at_i": 1700000000,
                    "_tags": ["story", "author_nathan", "story_1001"]
                },
                {
                    "objectID": "1002",
                    "author": "dmitry",
                    "comment_text": "<p>Immediate mode + retained layout feels great.</p>",
                    "story_id": 1001,
                    "story_title": "Introducing GPUI",
                    "story_url": "https://zed.dev/blog/gpui",
                    "created_at_i": 1700000120,
                    "_tags": ["comment", "author_dmitry", "story_1001"]
                }
            ],
            "nbHits": 2,
            "page": 0,
            "nbPages": 1
        }"#;

        let parsed: AlgoliaSearchResponse = serde_json::from_str(payload).unwrap();
        let results =
            parse_search_response(parsed, "gpui", SearchFilter::All, SearchSort::Relevance, 0)
                .unwrap();

        assert_eq!(results.query, "gpui");
        assert_eq!(results.filter, SearchFilter::All);
        assert_eq!(results.sort, SearchSort::Relevance);
        assert_eq!(results.hits.len(), 2);

        match &results.hits[0] {
            SearchHit::Story(story) => {
                assert_eq!(story.id, 1001);
                assert_eq!(story.title, "Introducing GPUI");
                assert_eq!(story.domain.as_deref(), Some("zed.dev"));
            }
            SearchHit::Comment(_) => panic!("expected SearchHit::Story"),
        }

        match &results.hits[1] {
            SearchHit::Comment(comment) => {
                assert_eq!(comment.id, 1002);
                assert_eq!(comment.story_id, 1001);
                assert_eq!(comment.story_title, "Introducing GPUI");
                assert_eq!(
                    comment.story_url.as_deref(),
                    Some("https://zed.dev/blog/gpui")
                );
                assert_eq!(comment.author, "dmitry");
                assert_eq!(
                    comment.text_html,
                    "<p>Immediate mode + retained layout feels great.</p>"
                );
            }
            SearchHit::Story(_) => panic!("expected SearchHit::Comment"),
        }
    }

    #[test]
    #[ignore = "hits live Algolia network"]
    fn live_algolia_top_feed_smoke_test() {
        let client = AlgoliaClient::new();
        let page = client.fetch_feed(FeedKind::Top, 0).unwrap();
        assert!(!page.stories.is_empty());
    }
}
