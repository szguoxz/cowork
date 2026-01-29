//! Context monitoring - simple functions for checking token usage
//!
//! No state tracking needed - just check the LLM-reported tokens against limits.

use crate::provider::{catalog, model_listing::get_model_context_limit};

/// Default threshold percentage for auto-compact (75%)
pub const AUTO_COMPACT_THRESHOLD: f64 = 0.75;

/// Minimum tokens remaining before forcing compaction
pub const MIN_REMAINING_TOKENS: usize = 20_000;

/// Get context limit for a provider/model
pub fn context_limit(provider_id: &str, model: Option<&str>) -> usize {
    // Check model-specific limits first
    if let Some(model) = model {
        if let Some(limit) = get_model_context_limit(provider_id, model) {
            if limit > 0 {
                return limit;
            }
        }
    }

    // Fall back to provider defaults from catalog
    let limit = catalog::context_window(provider_id).unwrap_or(128_000);

    // Guard against 0
    if limit == 0 {
        tracing::warn!(
            provider_id = %provider_id,
            model = ?model,
            "Context limit is 0 - returning fallback 128000"
        );
        return 128_000;
    }

    limit
}

/// Check if context should be compacted
///
/// Returns true if usage exceeds threshold or remaining tokens too low.
pub fn should_compact(input_tokens: u64, output_tokens: u64, limit: usize) -> bool {
    let used = input_tokens + output_tokens;
    let percentage = used as f64 / limit as f64;
    let remaining = (limit as u64).saturating_sub(used) as usize;

    percentage >= AUTO_COMPACT_THRESHOLD || remaining < MIN_REMAINING_TOKENS
}

/// Calculate context usage stats
pub fn usage_stats(input_tokens: u64, output_tokens: u64, limit: usize) -> ContextUsage {
    let used = input_tokens + output_tokens;
    let used_percentage = if limit > 0 {
        used as f64 / limit as f64
    } else {
        0.0
    };
    let remaining = (limit as u64).saturating_sub(used) as usize;

    ContextUsage {
        input_tokens,
        output_tokens,
        limit_tokens: limit,
        used_percentage,
        remaining_tokens: remaining,
        should_compact: should_compact(input_tokens, output_tokens, limit),
    }
}

/// Context usage statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub limit_tokens: usize,
    pub used_percentage: f64,
    pub remaining_tokens: usize,
    pub should_compact: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_compact_below_threshold() {
        // 50% usage - should not compact
        assert!(!should_compact(100_000, 0, 200_000));
    }

    #[test]
    fn test_should_compact_above_threshold() {
        // 80% usage - should compact
        assert!(should_compact(160_000, 0, 200_000));
    }

    #[test]
    fn test_should_compact_low_remaining() {
        // Less than 20k remaining - should compact
        assert!(should_compact(185_000, 0, 200_000));
    }

    #[test]
    fn test_context_limit_fallback() {
        let limit = context_limit("unknown_provider", None);
        assert_eq!(limit, 128_000);
    }

    #[test]
    fn test_usage_stats() {
        let usage = usage_stats(50_000, 1_000, 200_000);
        assert_eq!(usage.input_tokens, 50_000);
        assert_eq!(usage.output_tokens, 1_000);
        assert!(!usage.should_compact);
    }
}
