/// PeerScore type used for Peer Reputation
pub type PeerScore = f64;

/// Minimum allowed peer score before peer is banned
pub const MIN_PEER_SCORE: PeerScore = 0.0;
/// Default value for peer score
pub const DEFAULT_PEER_SCORE: PeerScore = 50.0;
/// Maximum value a Peer can reach with its PeerScore
pub const MAX_PEER_SCORE: PeerScore = 150.0;
/// Score by which we slowly decrease active peer reputation
pub const DECAY_PEER_SCORE: PeerScore = 0.9;

/// Types implementing this can report new PeerScore
pub trait PeerReport {
    /// Extracts PeerScore from the Report
    fn get_score_from_report(&self) -> PeerScore;
}

/// Example of negative PeerReport
#[derive(Debug, Clone)]
pub enum NegativePeerReport {
    /// Worst offense, peer should likely be banned after this
    Fatal,
    /// Minor offense, deduct few points
    Minor,
    /// Major offense, deduct reasonable amount of points
    Major,
}

impl PeerReport for NegativePeerReport {
    fn get_score_from_report(&self) -> PeerScore {
        match self {
            Self::Fatal => -MAX_PEER_SCORE - 10.0,
            Self::Major => -10.0,
            Self::Minor => -5.0,
        }
    }
}

/// Example of positive PeerReport
#[derive(Debug, Clone)]
pub enum PositivePeerReport {
    /// Minor positive feedback, increase reputation slightly
    Minor,
    /// Major positive feedback, increase reputation
    Major,
}

impl PeerReport for PositivePeerReport {
    fn get_score_from_report(&self) -> PeerScore {
        match self {
            Self::Major => 5.0,
            Self::Minor => 1.0,
        }
    }
}
