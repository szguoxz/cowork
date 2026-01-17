//! Browser automation tools

mod navigate;
mod screenshot;
mod interact;

pub use navigate::NavigateTo;
pub use screenshot::TakeScreenshot;
pub use interact::ClickElement;

use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared browser session state
pub struct BrowserSession {
    /// Whether browser is currently active
    pub active: bool,
    /// Current page URL
    pub current_url: Option<String>,
    /// Page title
    pub title: Option<String>,
}

impl Default for BrowserSession {
    fn default() -> Self {
        Self {
            active: false,
            current_url: None,
            title: None,
        }
    }
}

/// Browser controller for managing browser instances
pub struct BrowserController {
    session: Arc<Mutex<BrowserSession>>,
    headless: bool,
}

impl BrowserController {
    pub fn new(headless: bool) -> Self {
        Self {
            session: Arc::new(Mutex::new(BrowserSession::default())),
            headless,
        }
    }

    pub fn session(&self) -> Arc<Mutex<BrowserSession>> {
        Arc::clone(&self.session)
    }

    pub fn is_headless(&self) -> bool {
        self.headless
    }
}
