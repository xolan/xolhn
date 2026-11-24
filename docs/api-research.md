# Hacker News API Research & v1.0.0 Architecture

This document evaluates the free public APIs available for **XolHN** (`v1.0.0` read-only MVP: browsing feeds, reading comments, searching, and opening external links without login).

---

## 1. Candidate APIs Comparison

| API / Source | Base URL | Auth / Key | Documented Rate Limit | Latency / Freshness | Comment Tree Support | Best Fit |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Official HN Firebase API** | `https://hacker-news.firebaseio.com/v0/` | None | None documented (fair use) | Real-time (canonical; supports REST SSE) | Recursive `kids[]` ID fetches (N+1) | Authoritative feed lists, live scores, canonical reply order |
| **Algolia HN Search API** | `https://hn.algolia.com/api/v1/` | None | **10,000 req/hr per IP** (`429` on bursts) | ~1–3 min indexing lag | **Single-call pre-nested tree** via `/items/:id` | Fast comment tree loads, full-text search, chronological sort |
| **Official RSS** | `https://news.ycombinator.com/rss` | None | Fair use | Near real-time | None (stories only) | Not suitable for full client |
| **`hnrss.org`** | `https://hnrss.org/` | None | Fair use (wraps Algolia) | Inherits Algolia lag | Flattened feed items only | Optional external RSS export/integration |
| **HNPWA (`api.hnpwa.com` / `api.hackerwebapp.com`)** | Community proxies | None | N/A | Often degraded / defunct | Wrapper-dependent | **Avoid** (unmaintained / downtime & cert issues) |
| **HTML Scraping** | `https://news.ycombinator.com/` | None | Aggressive anti-scraping | Real-time | Brittle HTML DOM | **Avoid** for v1.0.0 (only needed later if login/voting/posting is added) |

---

## 2. Detailed API Breakdown

### A. Official Hacker News Firebase API
- **Base URL:** `https://hacker-news.firebaseio.com/v0/`
- **Rate Limits & Auth:** Free, no API key required, no hard rate limit published. Heavy uncached polling or high-concurrency scraping can still trigger fair-use throttling.
- **Core Endpoints:**
  - `topstories.json` (up to 500 top story IDs)
  - `newstories.json` (newest story IDs)
  - `beststories.json` (best-ranked story IDs)
  - `askstories.json` (up to 200 Ask HN IDs)
  - `showstories.json` (up to 200 Show HN IDs)
  - `jobstories.json` (up to 200 Job IDs)
  - `item/<id>.json` (single story, comment, poll, pollopt, or job)
  - `user/<username>.json` (public profile metadata)
  - `maxitem.json` (latest global item ID)
  - `updates.json` (recently changed `items` and `profiles`; also supports Firebase REST Server-Sent Events via `Accept: text/event-stream`)
- **Strengths:**
  - Canonical, authoritative source of truth for feed rankings and top-level/nested `kids` order.
  - Zero indexing delay.
  - Explicitly includes `deleted: true` and `dead: true` flags so thread structure stays intact.
- **Limitations:**
  - **N+1 fetch problem:** A feed page of 30 stories requires 1 list request + 30 item requests. A large discussion with 400 comments requires up to 401 requests if fetched strictly from Firebase.
  - **No search:** Cannot filter by keyword, domain, date range, or author.
  - **Legacy HTML bodies:** Comment and Ask HN `text` fields contain raw HN HTML (`<p>`, `<a>`, `<i>`, `<pre><code>`, HTML entities).

### B. Algolia HN Search API
- **Base URL:** `https://hn.algolia.com/api/v1/`
- **Rate Limits & Auth:** Free, no API key required, documented limit of **10,000 requests per hour per IP**. Excess bursts return HTTP `429 Too Many Requests`.
- **Core Endpoints:**
  - `GET /api/v1/items/:id` — Returns the story/comment and its **entire nested comment tree** (`children[]` recursively) in **1 HTTP request**.
  - `GET /api/v1/search?query=...&tags=...&numericFilters=...` — Relevance/popularity-weighted search (use `tags=front_page` for current front-page stories).
  - `GET /api/v1/search_by_date?query=...&tags=...&numericFilters=...` — Strictly chronological search (`created_at_i`).
  - Supported `tags`: `story`, `comment`, `poll`, `pollopt`, `show_hn`, `ask_hn`, `front_page`, `author_<USERNAME>`, `story_<ID>`.
  - Supported `numericFilters`: `created_at_i`, `points`, `num_comments` (`<`, `<=`, `=`, `>`, `>=`).
- **Strengths:**
  - Eliminates the Firebase N+1 problem when opening comment threads.
  - Instant full-text search across virtually all HN history, plus flat newest-comments views (`tags=comment,story_<ID>` on `search_by_date`).
- **Caveats & Gotchas:**
  - **1–3 minute propagation lag:** Brand-new stories or comments may return incomplete/404 briefly right after posting.
  - **Omitted deleted/dead nodes:** Killed or deleted items are generally omitted rather than surfaced with tombstones.
  - **Comment ordering nuances:** Algolia's nested `children` order does not always match HN's live weighted `kids` ordering on active threads.
  - **Pagination cap:** Standard search queries cap deep pagination around the first 1,000 hits (slice by `created_at_i` if deeper paging is needed).
  - **Missing canonical list parity:** Does not provide exact equivalents for Firebase's `beststories` or `jobstories` lists.

### C. RSS & Third-Party Community Wrappers
- **`news.ycombinator.com/rss`:** Front-page stories only; no comments or Ask/Show/Best feeds.
- **`hnrss.org`:** Useful community wrapper over Algolia supporting RSS/Atom/JSON (`?format=json`), but flattens nested replies and inherits Algolia's 1–3 minute lag.
- **`api.hnpwa.com` / `api.hackerwebapp.com`:** Unmaintained community proxies prone to outages and expired TLS certificates—**do not depend on hosted third-party instances**.

---

## 3. v1.0.0 Decision: Algolia-Only (Behind `HnApi` Trait)

For **v1.0.0**, XolHN uses the **Algolia HN Search API** (`https://hn.algolia.com/api/v1/`) as its single network backend, wrapped behind an internal `HnApi` trait in `xolhn-core` so UI and cache layers remain decoupled from the transport.

### Endpoint Mapping for All Feeds & Views

| XolHN View | Algolia Query |
| :--- | :--- |
| **Top (Front Page)** | `GET /api/v1/search?tags=front_page&hitsPerPage=30` |
| **New** | `GET /api/v1/search_by_date?tags=story&hitsPerPage=30&page=<n>` |
| **Best (time-windowed)** | `GET /api/v1/search?tags=story&numericFilters=created_at_i><ts>&hitsPerPage=30` |
| **Ask HN** | `GET /api/v1/search?tags=ask_hn&hitsPerPage=30` (or `/search_by_date` for newest) |
| **Show HN** | `GET /api/v1/search?tags=show_hn&hitsPerPage=30` (or `/search_by_date` for newest) |
| **Jobs** | `GET /api/v1/search_by_date?tags=job&hitsPerPage=30&page=<n>` |
| **Full Comment Tree** | `GET /api/v1/items/:id` |
| **Story / Comment Search** | `GET /api/v1/search?query=<q>` and `/api/v1/search_by_date?query=<q>` |

### Verified Algolia-Only Trade-offs

- **`front_page` caps at 30 items (`nbHits: 30`):** Represents page 1 of Hacker News rather than ranks 31–500. Deeper top-story browsing can pair with a time-windowed `search?tags=story` query.
- **Approximate `Best` ranking:** Algolia has no literal `best` tag, so `Best` is approximated by querying top-scored stories within a recent time window (`created_at_i > now - window`).
- **~1–3 minute lag & omitted dead/deleted items:** Very recent stories/comments may take 1–3 minutes to appear, and killed/deleted items are generally omitted rather than surfaced with tombstones; nested `children[]` ordering can also diverge slightly from HN's live weighted `kids[]` order.
- **Local caching & safe HTML rendering:** Cache feed pages and thread trees locally with a short TTL to stay well under the 10,000 req/hr IP limit, and parse HN's HTML tag subset (`<p>`, `<a>`, `<i>`, `<pre><code>`, entities) into native styled text (`gpui-kit` `TextView` / GPUI text spans).

---

## 4. Future Option: Hybrid Firebase + Algolia

Because `xolhn-core` abstracts data fetching behind `HnApi`, a future version can plug in **Official Firebase** (`/v0/*stories.json` + `/v0/item/<id>.json`) for canonical 500-item Top/Best lists, zero-lag brand-new items, and `deleted`/`dead` tombstones while keeping **Algolia** for 1-request comment trees and search—without changing `xolhn-ui`.

