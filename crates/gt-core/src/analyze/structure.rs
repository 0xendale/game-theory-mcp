//! Pareto efficiency, constant-sum detection, security levels, and welfare.

use crate::game::{PlayerId, Profile, Rational, StrategyId, ValidStrategicGame};
use crate::solve::pure_nash::solve_pure_nash;
use num_traits::Zero;
use serde::{Deserialize, Serialize};

/// A player's maxmin value: the best payoff they can guarantee themselves
/// regardless of what everyone else does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityLevel {
    pub player: PlayerId,
    pub value: Rational,
    pub maxmin_strategy: StrategyId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DominatedEquilibrium {
    pub equilibrium: Profile,
    pub dominated_by: Profile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureReport {
    pub pareto_frontier: Vec<Profile>,
    pub is_zero_sum: bool,
    pub is_constant_sum: bool,
    pub sum_constant: Option<Rational>,
    pub security_levels: Vec<SecurityLevel>,
    pub welfare_optima: Vec<Profile>,
    pub welfare_value: Rational,
    /// Pure equilibria that some other profile makes everyone weakly better off
    /// at, and someone strictly. This is the prisoner's-dilemma signature.
    pub dominated_equilibria: Vec<DominatedEquilibrium>,
    /// Welfare at the optimum minus welfare at the best pure equilibrium.
    /// `None` when there is no pure equilibrium.
    pub efficiency_gap: Option<Rational>,
}

pub fn analyze_structure(game: &ValidStrategicGame) -> StructureReport {
    let profiles: Vec<Profile> = game.profiles().collect();

    let pareto_frontier: Vec<Profile> = profiles
        .iter()
        .filter(|candidate| {
            !profiles
                .iter()
                .any(|other| pareto_dominates(game, other, candidate))
        })
        .cloned()
        .collect();

    let (is_constant_sum, sum_constant) = constant_sum(game, &profiles);
    let is_zero_sum = sum_constant.as_ref().is_some_and(Zero::is_zero);

    let security_levels = (0..game.n_players())
        .map(|p| security_level(game, &profiles, p))
        .collect();

    let welfare = |profile: &[StrategyId]| -> Rational { game.payoffs_at(profile).iter().sum() };
    let welfare_value = profiles
        .iter()
        .map(|p| welfare(p))
        .max()
        .expect("validation guarantees at least one profile");
    let welfare_optima: Vec<Profile> = profiles
        .iter()
        .filter(|p| welfare(p) == welfare_value)
        .cloned()
        .collect();

    let equilibria = solve_pure_nash(game).equilibria;

    let dominated_equilibria: Vec<DominatedEquilibrium> = equilibria
        .iter()
        .filter_map(|eq| {
            profiles
                .iter()
                .find(|other| pareto_dominates(game, other, eq))
                .map(|other| DominatedEquilibrium {
                    equilibrium: eq.clone(),
                    dominated_by: other.clone(),
                })
        })
        .collect();

    let efficiency_gap = equilibria
        .iter()
        .map(|eq| welfare(eq))
        .max()
        .map(|best_equilibrium_welfare| &welfare_value - best_equilibrium_welfare);

    StructureReport {
        pareto_frontier,
        is_zero_sum,
        is_constant_sum,
        sum_constant,
        security_levels,
        welfare_optima,
        welfare_value,
        dominated_equilibria,
        efficiency_gap,
    }
}

/// Does `a` make every player at least as well off as `b`, and someone
/// strictly better off?
pub fn pareto_dominates(game: &ValidStrategicGame, a: &[StrategyId], b: &[StrategyId]) -> bool {
    let ua = game.payoffs_at(a);
    let ub = game.payoffs_at(b);
    let all_weakly_better = ua.iter().zip(ub).all(|(x, y)| x >= y);
    let one_strictly_better = ua.iter().zip(ub).any(|(x, y)| x > y);
    all_weakly_better && one_strictly_better
}

fn constant_sum(game: &ValidStrategicGame, profiles: &[Profile]) -> (bool, Option<Rational>) {
    let mut sums = profiles
        .iter()
        .map(|p| -> Rational { game.payoffs_at(p).iter().sum() });
    let Some(first) = sums.next() else {
        return (false, None);
    };
    if sums.all(|s| s == first) {
        (true, Some(first))
    } else {
        (false, None)
    }
}

fn security_level(
    game: &ValidStrategicGame,
    profiles: &[Profile],
    player: PlayerId,
) -> SecurityLevel {
    let mut best_strategy = 0;
    let mut best_guarantee: Option<Rational> = None;

    for own in 0..game.n_strategies(player) {
        let worst = profiles
            .iter()
            .filter(|p| p[player] == own)
            .map(|p| game.payoff(p, player).clone())
            .min()
            .expect("every strategy appears in at least one profile");

        // `is_none_or` would read better but is newer than the crate's MSRV.
        let improves = match &best_guarantee {
            Some(current) => worst > *current,
            None => true,
        };
        if improves {
            best_guarantee = Some(worst);
            best_strategy = own;
        }
    }

    SecurityLevel {
        player,
        value: best_guarantee.expect("player has at least one strategy"),
        maxmin_strategy: best_strategy,
    }
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

    fn pd() -> ValidStrategicGame {
        game(vec![
            vec![[3.0, 3.0], [0.0, 4.0]],
            vec![[4.0, 0.0], [1.0, 1.0]],
        ])
    }

    fn int(n: i64) -> Rational {
        Rational::from_integer(n.into())
    }

    #[test]
    fn mutual_defection_is_pareto_dominated_by_mutual_cooperation() {
        assert!(pareto_dominates(&pd(), &[0, 0], &[1, 1]));
        assert!(!pareto_dominates(&pd(), &[1, 1], &[0, 0]));
    }

    #[test]
    fn the_pareto_frontier_excludes_mutual_defection() {
        let report = analyze_structure(&pd());
        assert!(!report.pareto_frontier.contains(&vec![1, 1]));
        assert!(report.pareto_frontier.contains(&vec![0, 0]));
        assert!(report.pareto_frontier.contains(&vec![0, 1]));
        assert!(report.pareto_frontier.contains(&vec![1, 0]));
    }

    #[test]
    fn the_prisoners_dilemma_equilibrium_is_reported_as_dominated() {
        let report = analyze_structure(&pd());
        assert_eq!(report.dominated_equilibria.len(), 1);
        assert_eq!(report.dominated_equilibria[0].equilibrium, vec![1, 1]);
        assert_eq!(report.dominated_equilibria[0].dominated_by, vec![0, 0]);
    }

    #[test]
    fn welfare_optimum_and_efficiency_gap_are_computed() {
        let report = analyze_structure(&pd());
        assert_eq!(report.welfare_optima, vec![vec![0, 0]]);
        assert_eq!(report.welfare_value, int(6));
        // Equilibrium welfare is 2; the optimum is 6.
        assert_eq!(report.efficiency_gap, Some(int(4)));
    }

    #[test]
    fn matching_pennies_is_recognised_as_zero_sum() {
        let mp = game(vec![
            vec![[1.0, -1.0], [-1.0, 1.0]],
            vec![[-1.0, 1.0], [1.0, -1.0]],
        ]);
        let report = analyze_structure(&mp);
        assert!(report.is_zero_sum);
        assert!(report.is_constant_sum);
        assert_eq!(report.sum_constant, Some(int(0)));
    }

    #[test]
    fn a_constant_sum_game_that_is_not_zero_sum_is_distinguished() {
        let g = game(vec![
            vec![[3.0, 7.0], [6.0, 4.0]],
            vec![[8.0, 2.0], [1.0, 9.0]],
        ]);
        let report = analyze_structure(&g);
        assert!(report.is_constant_sum);
        assert!(!report.is_zero_sum);
        assert_eq!(report.sum_constant, Some(int(10)));
    }

    #[test]
    fn the_prisoners_dilemma_is_not_constant_sum() {
        let report = analyze_structure(&pd());
        assert!(!report.is_constant_sum);
        assert!(!report.is_zero_sum);
        assert_eq!(report.sum_constant, None);
    }

    #[test]
    fn security_levels_are_the_maxmin_payoffs() {
        // Row's worst case: choosing Cooperate risks 0; Defect guarantees 1.
        let report = analyze_structure(&pd());
        let row = &report.security_levels[0];
        assert_eq!(row.value, int(1));
        assert_eq!(row.maxmin_strategy, 1);
    }

    #[test]
    fn a_game_with_no_equilibrium_has_no_efficiency_gap() {
        let mp = game(vec![
            vec![[1.0, -1.0], [-1.0, 1.0]],
            vec![[-1.0, 1.0], [1.0, -1.0]],
        ]);
        let report = analyze_structure(&mp);
        assert_eq!(report.efficiency_gap, None);
        assert!(report.dominated_equilibria.is_empty());
    }
}
