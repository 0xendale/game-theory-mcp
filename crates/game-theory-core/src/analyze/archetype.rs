//! Classifying a game against named patterns.
//!
//! Each archetype has a formal criterion, and the report states which criteria
//! were met and which failed. A game that matches nothing is `Unclassified` —
//! the classifier never picks the nearest label.

use crate::analyze::structure::{analyze_structure, pareto_dominates};
use crate::game::{Profile, ValidStrategicGame};
use crate::solve::dominance::{solve_dominance, DominanceMode};
use crate::solve::pure_nash::solve_pure_nash;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Archetype {
    PrisonersDilemma,
    StagHunt,
    Chicken,
    BattleOfTheSexes,
    MatchingPennies,
    Unclassified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchetypeReport {
    pub archetype: Archetype,
    pub criteria_met: Vec<String>,
    pub criteria_failed: Vec<String>,
}

pub fn classify(game: &ValidStrategicGame) -> ArchetypeReport {
    let mut met = Vec::new();
    let mut failed = Vec::new();

    let structure = analyze_structure(game);
    let equilibria = solve_pure_nash(game).equilibria;
    let dominance = solve_dominance(game, DominanceMode::Strict);
    let is_two_by_two =
        game.n_players() == 2 && game.n_strategies(0) == 2 && game.n_strategies(1) == 2;

    // Prisoner's Dilemma: any player count. A dominant-strategy equilibrium
    // that some other profile Pareto-dominates.
    if let Some(dominant) = &dominance.unique_profile {
        met.push(format!(
            "iterated strict dominance yields the unique profile {dominant:?}"
        ));
        if structure
            .dominated_equilibria
            .iter()
            .any(|d| &d.equilibrium == dominant)
        {
            met.push(
                "that dominant-strategy equilibrium is Pareto-dominated by another profile"
                    .to_string(),
            );
            return ArchetypeReport {
                archetype: Archetype::PrisonersDilemma,
                criteria_met: met,
                criteria_failed: failed,
            };
        }
        failed.push(
            "the dominant-strategy equilibrium is not Pareto-dominated, so this is not a \
             prisoner's dilemma"
                .to_string(),
        );
    } else {
        failed.push("no dominant-strategy equilibrium, so not a prisoner's dilemma".to_string());
    }

    if !is_two_by_two {
        failed
            .push("the remaining archetypes are 2x2 patterns and this game is not 2x2".to_string());
        return ArchetypeReport {
            archetype: Archetype::Unclassified,
            criteria_met: met,
            criteria_failed: failed,
        };
    }

    // Matching Pennies: zero-sum with no pure equilibrium.
    if structure.is_zero_sum && equilibria.is_empty() {
        met.push("zero-sum with no pure-strategy equilibrium".to_string());
        return ArchetypeReport {
            archetype: Archetype::MatchingPennies,
            criteria_met: met,
            criteria_failed: failed,
        };
    }

    if equilibria.len() != 2 {
        failed.push(format!(
            "stag hunt, chicken, and battle of the sexes each need exactly two pure \
             equilibria; this game has {}",
            equilibria.len()
        ));
        return ArchetypeReport {
            archetype: Archetype::Unclassified,
            criteria_met: met,
            criteria_failed: failed,
        };
    }

    let (a, b) = (&equilibria[0], &equilibria[1]);

    // Stag Hunt: one equilibrium Pareto-dominates the other.
    if pareto_dominates(game, a, b) || pareto_dominates(game, b, a) {
        met.push("two pure equilibria, one Pareto-dominating the other".to_string());
        return ArchetypeReport {
            archetype: Archetype::StagHunt,
            criteria_met: met,
            criteria_failed: failed,
        };
    }
    failed.push("neither equilibrium Pareto-dominates the other, so not a stag hunt".to_string());

    // Chicken: the two equilibria are off-diagonal, and the profile where both
    // players pick their aggressive strategy is worst for both.
    if is_off_diagonal(a, b) {
        let aggressive = mutual_aggression_profile(a, b);
        let worst_for_both = game.profiles().all(|other| {
            other == aggressive
                || game.payoff(&other, 0) >= game.payoff(&aggressive, 0)
                    && game.payoff(&other, 1) >= game.payoff(&aggressive, 1)
        });
        if worst_for_both {
            met.push(
                "two off-diagonal equilibria, with mutual aggression worst for both players"
                    .to_string(),
            );
            return ArchetypeReport {
                archetype: Archetype::Chicken,
                criteria_met: met,
                criteria_failed: failed,
            };
        }
        failed.push("the off-diagonal profile is not worst for both, so not chicken".to_string());
    }

    // Battle of the Sexes: the two equilibria are on-diagonal and the players
    // rank them oppositely.
    if !is_off_diagonal(a, b) {
        let row_prefers_a = game.payoff(a, 0) > game.payoff(b, 0);
        let col_prefers_a = game.payoff(a, 1) > game.payoff(b, 1);
        if row_prefers_a != col_prefers_a {
            met.push("two coordination equilibria that the players rank oppositely".to_string());
            return ArchetypeReport {
                archetype: Archetype::BattleOfTheSexes,
                criteria_met: met,
                criteria_failed: failed,
            };
        }
        failed.push(
            "both players rank the two equilibria the same way, so not battle of the sexes"
                .to_string(),
        );
    }

    ArchetypeReport {
        archetype: Archetype::Unclassified,
        criteria_met: met,
        criteria_failed: failed,
    }
}

/// Two 2-player equilibria are off-diagonal when they disagree in both
/// coordinates — e.g. (0,1) and (1,0).
fn is_off_diagonal(a: &Profile, b: &Profile) -> bool {
    a[0] != b[0] && a[1] != b[1] && a[0] != a[1] && b[0] != b[1]
}

/// The profile where both players take the strategy they use in the equilibrium
/// they prefer. For off-diagonal equilibria (x, y) and (y, x) in a 2x2 game,
/// `solve_pure_nash` enumerates ascending, so `a` is (0, 1) and `b` is (1, 0):
/// row's aggressive strategy is `b[0]` and column's is `a[1]`.
fn mutual_aggression_profile(a: &Profile, b: &Profile) -> Profile {
    vec![b[0], a[1]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{MatrixForm, PayoffKind, StrategicGame};

    fn game(m: Vec<Vec<[f64; 2]>>) -> ValidStrategicGame {
        let rows = m.len();
        let cols = m[0].len();
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: (0..rows).map(|i| format!("r{i}")).collect(),
            col_strategies: (0..cols).map(|j| format!("c{j}")).collect(),
            payoff_matrix: m,
            payoff_kind: PayoffKind::Cardinal,
        };
        ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap()
    }

    #[test]
    fn identifies_the_prisoners_dilemma() {
        let g = game(vec![
            vec![[3.0, 3.0], [0.0, 4.0]],
            vec![[4.0, 0.0], [1.0, 1.0]],
        ]);
        let report = classify(&g);
        assert_eq!(report.archetype, Archetype::PrisonersDilemma);
        assert!(!report.criteria_met.is_empty(), "must show its reasoning");
    }

    #[test]
    fn identifies_the_stag_hunt() {
        // Two equilibria; (Stag, Stag) Pareto-dominates (Hare, Hare);
        // no dominant strategy.
        let g = game(vec![
            vec![[4.0, 4.0], [0.0, 3.0]],
            vec![[3.0, 0.0], [3.0, 3.0]],
        ]);
        let report = classify(&g);
        assert_eq!(report.archetype, Archetype::StagHunt);
    }

    #[test]
    fn identifies_chicken() {
        // Two asymmetric equilibria; mutual aggression is worst for both.
        let g = game(vec![
            vec![[3.0, 3.0], [2.0, 4.0]],
            vec![[4.0, 2.0], [1.0, 1.0]],
        ]);
        let report = classify(&g);
        assert_eq!(report.archetype, Archetype::Chicken);
    }

    #[test]
    fn identifies_battle_of_the_sexes() {
        let g = game(vec![
            vec![[2.0, 1.0], [0.0, 0.0]],
            vec![[0.0, 0.0], [1.0, 2.0]],
        ]);
        let report = classify(&g);
        assert_eq!(report.archetype, Archetype::BattleOfTheSexes);
    }

    #[test]
    fn identifies_matching_pennies() {
        let g = game(vec![
            vec![[1.0, -1.0], [-1.0, 1.0]],
            vec![[-1.0, 1.0], [1.0, -1.0]],
        ]);
        let report = classify(&g);
        assert_eq!(report.archetype, Archetype::MatchingPennies);
    }

    #[test]
    fn an_unremarkable_game_is_left_unclassified_rather_than_guessed() {
        let g = game(vec![
            vec![[1.0, 1.0], [1.0, 1.0]],
            vec![[1.0, 1.0], [1.0, 1.0]],
        ]);
        let report = classify(&g);
        assert_eq!(report.archetype, Archetype::Unclassified);
        assert!(
            !report.criteria_failed.is_empty(),
            "must say why nothing matched"
        );
    }

    #[test]
    fn a_three_player_dominated_equilibrium_still_classifies_as_a_dilemma() {
        use crate::game::{Outcome, Player};
        // Three-player public goods game: contributing costs 2 and gives every
        // player 1. Defecting is dominant; universal contribution is better.
        let mut outcomes = Vec::new();
        for a in 0..2usize {
            for b in 0..2usize {
                for c in 0..2usize {
                    let contributors = [a, b, c].iter().filter(|&&x| x == 0).count() as f64;
                    let payoff = |own: usize| contributors - if own == 0 { 2.0 } else { 0.0 };
                    outcomes.push(Outcome {
                        profile: vec![a, b, c],
                        payoffs: vec![payoff(a), payoff(b), payoff(c)],
                    });
                }
            }
        }
        let g = ValidStrategicGame::validate(StrategicGame {
            players: (0..3)
                .map(|id| Player {
                    id,
                    name: format!("P{id}"),
                })
                .collect(),
            strategies: vec![vec!["Contribute".into(), "Defect".into()]; 3],
            outcomes,
            payoff_kind: PayoffKind::Cardinal,
        })
        .unwrap();

        let report = classify(&g);
        assert_eq!(report.archetype, Archetype::PrisonersDilemma);
    }

    #[test]
    fn a_non_two_by_two_game_cannot_match_the_two_by_two_patterns() {
        let g = game(vec![
            vec![[1.0, 1.0], [0.0, 0.0], [0.0, 0.0]],
            vec![[0.0, 0.0], [1.0, 1.0], [0.0, 0.0]],
        ]);
        let report = classify(&g);
        assert_ne!(report.archetype, Archetype::StagHunt);
        assert_ne!(report.archetype, Archetype::Chicken);
        assert_ne!(report.archetype, Archetype::BattleOfTheSexes);
    }
}
