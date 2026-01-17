//! Web tools for fetching and processing web content

mod fetch;
mod search;

pub use fetch::WebFetch;
pub use search::{SearchResult, WebSearch, WebSearchConfig};
