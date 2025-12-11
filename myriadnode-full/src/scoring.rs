use std::collections::HashMap;
use tracing::debug;

/// Weights for adapter scoring algorithm
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub latency: f64,
    pub bandwidth: f64,
    pub reliability: f64,
    pub power: f64,
    pub privacy: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            latency: 0.25,     // 25% weight on latency
            bandwidth: 0.20,   // 20% weight on bandwidth
            reliability: 0.30, // 30% weight on reliability
            power: 0.10,       // 10% weight on power consumption
            privacy: 0.15,     // 15% weight on privacy/anonymity
        }
    }
}

impl ScoringWeights {
    /// Battery-optimized weights (prioritize low power consumption)
    pub fn battery_optimized() -> Self {
        Self {
            latency: 0.15,
            bandwidth: 0.10,
            reliability: 0.25,
            power: 0.40, // Higher weight on power for battery mode
            privacy: 0.10,
        }
    }

    /// Performance-optimized weights (prioritize speed and bandwidth)
    pub fn performance_optimized() -> Self {
        Self {
            latency: 0.35,   // Higher weight on latency
            bandwidth: 0.30, // Higher weight on bandwidth
            reliability: 0.20,
            power: 0.05,
            privacy: 0.10, // Lower priority for performance mode
        }
    }

    /// Reliability-optimized weights (prioritize stable connections)
    pub fn reliability_optimized() -> Self {
        Self {
            latency: 0.10,
            bandwidth: 0.10,
            reliability: 0.65, // Much higher weight on reliability
            power: 0.05,
            privacy: 0.10,
        }
    }

    /// Privacy-optimized weights (prioritize anonymous/non-IP adapters for SENSITIVE messages)
    pub fn privacy_optimized() -> Self {
        Self {
            latency: 0.10,   // Lower priority - privacy over speed
            bandwidth: 0.05, // Lower priority
            reliability: 0.20,
            power: 0.10,
            privacy: 0.55, // 55% weight on privacy for sensitive messages
        }
    }

    /// Validate that weights sum to 1.0 (or very close)
    pub fn is_valid(&self) -> bool {
        let sum = self.latency + self.bandwidth + self.reliability + self.power + self.privacy;
        (sum - 1.0).abs() < 0.01
    }

    /// Normalize weights to ensure they sum to 1.0
    pub fn normalize(&mut self) {
        let sum = self.latency + self.bandwidth + self.reliability + self.power + self.privacy;
        if sum > 0.0 {
            self.latency /= sum;
            self.bandwidth /= sum;
            self.reliability /= sum;
            self.power /= sum;
            self.privacy /= sum;
        }
    }
}

/// Raw metrics for an adapter
#[derive(Debug, Clone)]
pub struct AdapterMetrics {
    pub latency_ms: f64,
    pub bandwidth_bps: u64,
    pub reliability: f64,       // 0.0 to 1.0
    pub power_consumption: f64, // Relative scale 0.0 (low) to 1.0 (high)
    pub privacy_level: f64,     // 0.0 (traceable/IP) to 1.0 (anonymous/non-IP)
}

/// Calculated score for an adapter
#[derive(Debug, Clone)]
pub struct AdapterScore {
    pub adapter_id: String,
    pub total_score: f64,
    pub latency_score: f64,
    pub bandwidth_score: f64,
    pub reliability_score: f64,
    pub power_score: f64,
    pub privacy_score: f64,
}

/// Adapter scoring calculator
#[derive(Clone)]
pub struct AdapterScorer {
    weights: ScoringWeights,
    // Reference values for normalization
    max_bandwidth_bps: u64,
    max_latency_ms: f64,
}

impl AdapterScorer {
    pub fn new(weights: ScoringWeights) -> Self {
        Self {
            weights,
            max_bandwidth_bps: 100_000_000, // 100 Mbps as baseline
            max_latency_ms: 1000.0,         // 1 second as baseline
        }
    }

    pub fn new_with_defaults() -> Self {
        Self::new(ScoringWeights::default())
    }

    /// Calculate score for a single adapter
    pub fn calculate_score(&self, adapter_id: String, metrics: &AdapterMetrics) -> AdapterScore {
        // Calculate individual scores (0.0 to 1.0)
        let latency_score = self.score_latency(metrics.latency_ms);
        let bandwidth_score = self.score_bandwidth(metrics.bandwidth_bps);
        let reliability_score = metrics.reliability; // Already 0.0 to 1.0
        let power_score = self.score_power(metrics.power_consumption);
        let privacy_score = metrics.privacy_level; // Already 0.0 to 1.0

        // Calculate weighted total score
        let total_score = (latency_score * self.weights.latency)
            + (bandwidth_score * self.weights.bandwidth)
            + (reliability_score * self.weights.reliability)
            + (power_score * self.weights.power)
            + (privacy_score * self.weights.privacy);

        debug!(
            "Adapter '{}' score: {:.3} (lat={:.3}, bw={:.3}, rel={:.3}, pwr={:.3}, priv={:.3})",
            adapter_id,
            total_score,
            latency_score,
            bandwidth_score,
            reliability_score,
            power_score,
            privacy_score
        );

        AdapterScore {
            adapter_id,
            total_score,
            latency_score,
            bandwidth_score,
            reliability_score,
            power_score,
            privacy_score,
        }
    }

    /// Calculate scores for multiple adapters and return them sorted by score (highest first)
    pub fn rank_adapters(&self, adapters: HashMap<String, AdapterMetrics>) -> Vec<AdapterScore> {
        let mut scores: Vec<AdapterScore> = adapters
            .into_iter()
            .map(|(id, metrics)| self.calculate_score(id, &metrics))
            .collect();

        // Sort by total score descending (highest score first)
        scores.sort_by(|a, b| {
            b.total_score
                .partial_cmp(&a.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        scores
    }

    /// Get the best adapter from a set of candidates
    pub fn get_best_adapter(
        &self,
        adapters: HashMap<String, AdapterMetrics>,
    ) -> Option<AdapterScore> {
        self.rank_adapters(adapters).into_iter().next()
    }

    /// Score latency (lower is better, so we invert)
    fn score_latency(&self, latency_ms: f64) -> f64 {
        if latency_ms <= 0.0 {
            return 1.0;
        }

        // Use exponential decay for latency scoring
        // Score drops to 0.5 at max_latency_ms/2, approaches 0 at max_latency_ms
        let normalized = latency_ms / self.max_latency_ms;
        (1.0 - normalized).clamp(0.0, 1.0)
    }

    /// Score bandwidth (higher is better)
    fn score_bandwidth(&self, bandwidth_bps: u64) -> f64 {
        let normalized = bandwidth_bps as f64 / self.max_bandwidth_bps as f64;
        normalized.min(1.0)
    }

    /// Score power consumption (lower is better, so we invert)
    fn score_power(&self, power_consumption: f64) -> f64 {
        // power_consumption is 0.0 (low) to 1.0 (high)
        // We want score to be 1.0 (low power) to 0.0 (high power)
        (1.0 - power_consumption).clamp(0.0, 1.0)
    }

    /// Update weights (useful for dynamic adjustment)
    pub fn set_weights(&mut self, weights: ScoringWeights) {
        self.weights = weights;
    }

    pub fn get_weights(&self) -> &ScoringWeights {
        &self.weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_weights_valid() {
        let weights = ScoringWeights::default();
        assert!(weights.is_valid());
    }

    #[test]
    fn test_weight_normalization() {
        let mut weights = ScoringWeights {
            latency: 2.0,
            bandwidth: 2.0,
            reliability: 2.0,
            power: 2.0,
            privacy: 2.0,
        };
        weights.normalize();
        assert!(weights.is_valid());
        assert!((weights.latency - 0.20).abs() < 0.01); // 2.0 / 10.0 = 0.20
    }

    #[test]
    fn test_adapter_scoring() {
        let scorer = AdapterScorer::new_with_defaults();

        let metrics = AdapterMetrics {
            latency_ms: 50.0,
            bandwidth_bps: 10_000_000, // 10 Mbps
            reliability: 0.95,
            power_consumption: 0.3,
            privacy_level: 0.5, // Medium privacy
        };

        let score = scorer.calculate_score("test_adapter".to_string(), &metrics);

        assert!(score.total_score >= 0.0 && score.total_score <= 1.0);
        assert!(score.latency_score >= 0.0 && score.latency_score <= 1.0);
        assert!(score.bandwidth_score >= 0.0 && score.bandwidth_score <= 1.0);
        assert!(score.reliability_score >= 0.0 && score.reliability_score <= 1.0);
        assert!(score.power_score >= 0.0 && score.power_score <= 1.0);
        assert!(score.privacy_score >= 0.0 && score.privacy_score <= 1.0);
    }

    #[test]
    fn test_adapter_ranking() {
        let scorer = AdapterScorer::new_with_defaults();

        let mut adapters = HashMap::new();

        // Fast adapter (low latency, high bandwidth, low privacy - IP-based)
        adapters.insert(
            "fast".to_string(),
            AdapterMetrics {
                latency_ms: 10.0,
                bandwidth_bps: 50_000_000,
                reliability: 0.90,
                power_consumption: 0.7,
                privacy_level: 0.2, // Low privacy (IP-based)
            },
        );

        // Reliable adapter (high reliability, moderate speed, medium privacy)
        adapters.insert(
            "reliable".to_string(),
            AdapterMetrics {
                latency_ms: 100.0,
                bandwidth_bps: 10_000_000,
                reliability: 0.99,
                power_consumption: 0.5,
                privacy_level: 0.5, // Medium privacy
            },
        );

        // Power-efficient adapter (low power, slower, high privacy - Bluetooth mesh)
        adapters.insert(
            "efficient".to_string(),
            AdapterMetrics {
                latency_ms: 150.0,
                bandwidth_bps: 1_000_000,
                reliability: 0.85,
                power_consumption: 0.2,
                privacy_level: 0.8, // High privacy (non-IP)
            },
        );

        let ranked = scorer.rank_adapters(adapters);

        assert_eq!(ranked.len(), 3);
        // Verify scores are in descending order
        assert!(ranked[0].total_score >= ranked[1].total_score);
        assert!(ranked[1].total_score >= ranked[2].total_score);
    }

    #[test]
    fn test_best_adapter_selection() {
        let scorer = AdapterScorer::new_with_defaults();

        let mut adapters = HashMap::new();
        adapters.insert(
            "good".to_string(),
            AdapterMetrics {
                latency_ms: 20.0,
                bandwidth_bps: 30_000_000,
                reliability: 0.95,
                power_consumption: 0.4,
                privacy_level: 0.6,
            },
        );
        adapters.insert(
            "bad".to_string(),
            AdapterMetrics {
                latency_ms: 500.0,
                bandwidth_bps: 100_000,
                reliability: 0.50,
                power_consumption: 0.9,
                privacy_level: 0.3,
            },
        );

        let best = scorer.get_best_adapter(adapters).unwrap();
        assert_eq!(best.adapter_id, "good");
    }

    #[test]
    fn test_battery_optimized_weights() {
        let weights = ScoringWeights::battery_optimized();
        assert!(weights.is_valid());
        assert!(weights.power > 0.30); // Should prioritize power
    }

    #[test]
    fn test_performance_optimized_weights() {
        let weights = ScoringWeights::performance_optimized();
        assert!(weights.is_valid());
        assert!(weights.latency + weights.bandwidth > 0.60); // Should prioritize speed
    }

    #[test]
    fn test_privacy_optimized_weights() {
        let weights = ScoringWeights::privacy_optimized();
        assert!(weights.is_valid());
        assert!(weights.privacy > 0.50); // Should prioritize privacy for SENSITIVE messages
    }

    #[test]
    fn test_privacy_scoring_for_sensitive_messages() {
        // Use privacy-optimized weights for SENSITIVE messages
        let scorer = AdapterScorer::new(ScoringWeights::privacy_optimized());

        let mut adapters = HashMap::new();

        // IP-based adapter (fast but low privacy)
        adapters.insert(
            "ethernet".to_string(),
            AdapterMetrics {
                latency_ms: 10.0,
                bandwidth_bps: 100_000_000,
                reliability: 0.99,
                power_consumption: 0.5,
                privacy_level: 0.1, // Low privacy - IP traceable
            },
        );

        // I2P adapter (slower but high privacy)
        adapters.insert(
            "i2p".to_string(),
            AdapterMetrics {
                latency_ms: 500.0,
                bandwidth_bps: 1_000_000,
                reliability: 0.85,
                power_consumption: 0.6,
                privacy_level: 0.95, // High privacy - onion routing
            },
        );

        // Bluetooth mesh (medium speed, high privacy)
        adapters.insert(
            "bluetooth".to_string(),
            AdapterMetrics {
                latency_ms: 150.0,
                bandwidth_bps: 3_000_000,
                reliability: 0.90,
                power_consumption: 0.3,
                privacy_level: 0.80, // High privacy - local mesh, no IP
            },
        );

        let best = scorer.get_best_adapter(adapters).unwrap();

        // For SENSITIVE messages, should prefer i2p or bluetooth over ethernet
        assert_ne!(best.adapter_id, "ethernet");
        assert!(best.privacy_score > 0.75); // Should have high privacy score
    }
}
