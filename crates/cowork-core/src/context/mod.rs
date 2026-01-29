//! Context management for agent execution
//!
//! This module provides:
//! - Token counting and context compaction (summarizer)
//! - Context limit checking (monitor)
//! - Project context gathering (gather)

pub mod gather;
pub mod monitor;
pub mod summarizer;

pub use gather::{ContextGatherer, MemoryFile, MemoryHierarchy, MemoryTier, ProjectContext};
pub use monitor::{context_limit, should_compact, usage_stats, ContextUsage};
pub use summarizer::{compact, CompactResult};
