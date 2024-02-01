/// PeerScore type used for Peer Reputation
pub type AppScore = f64;

/// Minimum allowed peer score before peer is banned
pub const MIN_APP_SCORE: AppScore = -50.0;
/// Default value for peer score
pub const DEFAULT_APP_SCORE: AppScore = 0.0;
/// Maximum value a Peer can reach with its PeerScore
pub const MAX_APP_SCORE: AppScore = 150.0;
/// Score by which we slowly decrease active peer reputation
pub const DECAY_APP_SCORE: AppScore = 0.9;

/// Types implementing this can report new PeerScore
pub trait PeerReport {
    /// Extracts PeerScore from the Report
    fn get_score_from_report(&self) -> AppScore;
}

impl PeerReport for AppScore {
    fn get_score_from_report(&self) -> AppScore {
        *self
    }
}
