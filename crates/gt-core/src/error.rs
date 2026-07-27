//! Typed failures. Every variant names what went wrong and what to do instead.

use crate::game::{PlayerId, Profile};
use thiserror::Error;

/// A single validation problem. Validation collects all of them rather than
/// stopping at the first, so a caller can fix a malformed game in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    NoPlayers,
    PlayerIdMismatch,
    StrategyListArityMismatch,
    EmptyStrategySet,
    DuplicateStrategyLabel,
    PayoffArityMismatch,
    ProfileArityMismatch,
    StrategyIndexOutOfRange,
    DuplicateProfile,
    MissingProfile,
    NonFinitePayoff,
    // Extensive form.
    EmptyTree,
    RootOutOfRange,
    ChildOutOfRange,
    UnreachableNode,
    NotATree,
    EmptyActionSet,
    DuplicateActionLabel,
    TerminalPayoffArityMismatch,
    InformationSetNotPartition,
    InformationSetOnTerminal,
    InformationSetMixedPlayers,
    InformationSetActionCountMismatch,
}

#[derive(Debug, Clone, Error)]
pub enum GtError {
    #[error("game is not well formed: {} problem(s) found", .diagnostics.len())]
    InvalidGame { diagnostics: Vec<Diagnostic> },

    #[error("game too large: {field} is {actual}, limit is {limit}")]
    GameTooLarge {
        field: &'static str,
        limit: usize,
        actual: usize,
    },

    #[error(
        "{tool} takes expectations over payoffs, which is meaningless for ordinal \
         payoffs; supply cardinal (von Neumann-Morgenstern) utilities instead"
    )]
    OrdinalPayoffsRejected { tool: &'static str },

    #[error(
        "mixed-strategy Nash equilibrium is supported for 2 players only, this game \
         has {players}; use solve_pure_nash or solve_dominance instead"
    )]
    NPlayerMixedUnsupported { players: usize },

    #[error(
        "information set {information_set} contains more than one node; games of \
         imperfect information are not supported in this version"
    )]
    ImperfectInformationUnsupported { information_set: usize },

    #[error("expected a {expected} game, got a {actual} game")]
    WrongGameForm {
        expected: &'static str,
        actual: &'static str,
    },

    #[error("player {player} has no strategy at profile {profile:?}")]
    UnknownProfile { player: PlayerId, profile: Profile },

    #[error("player {player}'s mixed strategy is not a probability distribution: {reason}")]
    InvalidMixedStrategy { player: PlayerId, reason: String },

    #[error(
        "grim-trigger analysis needs a pure-strategy Nash equilibrium of the stage \
         game to revert to, and this game has none; use solve_mixed_nash to find a \
         mixed equilibrium, which this version cannot use as a punishment"
    )]
    NoPureNashForPunishment,

    #[error("discount factor {value} is outside [0, 1)")]
    InvalidDiscountFactor { value: String },
}

impl GtError {
    /// Convenience for the common single-diagnostic case.
    pub fn invalid(code: DiagnosticCode, message: impl Into<String>) -> Self {
        GtError::InvalidGame {
            diagnostics: vec![Diagnostic {
                code,
                message: message.into(),
            }],
        }
    }
}
