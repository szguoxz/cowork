//! Web tools for fetching and processing web content

mod fetch;
mod search;

pub use fetch::WebFetch;
pub use search::{supports_native_search, SearchResult, WebSearch, NATIVE_SEARCH_PROVIDERS};
