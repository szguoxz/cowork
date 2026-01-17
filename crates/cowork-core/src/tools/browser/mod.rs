//! Browser automation tools
//!
//! Provides tools for browser automation using chromiumoxide.
//! These tools allow navigating to URLs, taking screenshots, and interacting
//! with web pages.

mod navigate;
mod screenshot;
mod interact;

pub use navigate::NavigateTo;
pub use screenshot::TakeScreenshot;
pub use interact::{ClickElement, TypeText, GetPageContent};

use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "browser")]
use chromiumoxide::{Browser, BrowserConfig, Page};
#[cfg(feature = "browser")]
use futures::StreamExt;

/// Shared browser session state
pub struct BrowserSession {
    /// Whether browser is currently active
    pub active: bool,
    /// Current page URL
    pub current_url: Option<String>,
    /// Page title
    pub title: Option<String>,
    /// Chromium browser instance
    #[cfg(feature = "browser")]
    pub browser: Option<Browser>,
    /// Current active page
    #[cfg(feature = "browser")]
    pub page: Option<Page>,
    /// Whether running in headless mode
    pub headless: bool,
}

impl Default for BrowserSession {
    fn default() -> Self {
        Self {
            active: false,
            current_url: None,
            title: None,
            #[cfg(feature = "browser")]
            browser: None,
            #[cfg(feature = "browser")]
            page: None,
            headless: true,
        }
    }
}

impl BrowserSession {
    /// Create a new browser session
    pub fn new(headless: bool) -> Self {
        Self {
            headless,
            ..Default::default()
        }
    }

    /// Ensure browser is started
    #[cfg(feature = "browser")]
    pub async fn ensure_browser(&mut self) -> Result<(), crate::error::Error> {
        use crate::error::ToolError;

        if self.browser.is_some() && self.active {
            return Ok(());
        }

        let config = BrowserConfig::builder()
            .with_head() // Will be headless if headless flag is set
            .build()
            .map_err(|e| crate::error::Error::Tool(ToolError::ExecutionFailed(format!("Failed to create browser config: {}", e))))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| crate::error::Error::Tool(ToolError::ExecutionFailed(format!("Failed to launch browser: {}", e))))?;

        // Spawn handler task
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if event.is_err() {
                    break;
                }
            }
        });

        self.browser = Some(browser);
        self.active = true;

        Ok(())
    }

    /// Get or create a new page
    #[cfg(feature = "browser")]
    pub async fn get_page(&mut self) -> Result<&Page, crate::error::Error> {
        use crate::error::ToolError;

        self.ensure_browser().await?;

        if self.page.is_none() {
            let browser = self.browser.as_ref()
                .ok_or_else(|| crate::error::Error::Tool(ToolError::ExecutionFailed("Browser not initialized".to_string())))?;

            let page = browser.new_page("about:blank")
                .await
                .map_err(|e| crate::error::Error::Tool(ToolError::ExecutionFailed(format!("Failed to create page: {}", e))))?;

            self.page = Some(page);
        }

        self.page.as_ref()
            .ok_or_else(|| crate::error::Error::Tool(ToolError::ExecutionFailed("Page not available".to_string())))
    }

    /// Close the browser
    #[cfg(feature = "browser")]
    pub async fn close(&mut self) {
        self.page = None;
        self.browser = None;
        self.active = false;
        self.current_url = None;
        self.title = None;
    }
}

/// Browser controller for managing browser instances
pub struct BrowserController {
    session: Arc<Mutex<BrowserSession>>,
}

impl BrowserController {
    pub fn new(headless: bool) -> Self {
        Self {
            session: Arc::new(Mutex::new(BrowserSession::new(headless))),
        }
    }

    pub fn session(&self) -> Arc<Mutex<BrowserSession>> {
        Arc::clone(&self.session)
    }

    /// Create all browser tools
    pub fn create_tools(&self) -> Vec<Arc<dyn crate::tools::Tool>> {
        vec![
            Arc::new(NavigateTo::new(self.session())),
            Arc::new(TakeScreenshot::new(self.session())),
            Arc::new(ClickElement::new(self.session())),
            Arc::new(TypeText::new(self.session())),
            Arc::new(GetPageContent::new(self.session())),
        ]
    }
}

impl Default for BrowserController {
    fn default() -> Self {
        Self::new(true)
    }
}
