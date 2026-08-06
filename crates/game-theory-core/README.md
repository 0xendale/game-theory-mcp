# game-theory-core

Exact game-theoretic computation. No I/O, no async, no protocol handling.

`game-theory-mcp` is the MCP adapter over this crate; this crate is usable on
its own as a library.

## What it does

Strategic (normal) form:

| Function | Concept |
|---|---|
| `ValidStrategicGame::validate` | structural validation, limits, exact payoff table |
| `solve_dominance` | iterated deletion of strictly or weakly dominated strategies |
| `solve_pure_nash` | all pure-strategy Nash equilibria, any player count |
| `solve_mixed_nash` | all mixed equilibria of a non-degenerate 2-player game |
| `strictly_dominated_by_mixture` | is one strategy strictly dominated by a mixture of the others? |
| `verify_equilibrium` | confirm or refute a claimed pure or dominant-strategy equilibrium |
| `verify_mixed_nash` | confirm or refute a claimed mixed equilibrium |
| `analyze_structure` | Pareto frontier, constant-sum, security levels, welfare |
| `classify` | prisoner's dilemma, stag hunt, chicken, battle of the sexes, matching pennies |
| `analyze_repeated_game` | critical discount factor sustaining a profile under grim trigger |

Exact arithmetic primitives:

| Function | Concept |
|---|---|
| `solve_linear_system` | Gaussian elimination over rationals |
| `solve_lp` | two-phase primal simplex over rationals, Bland's rule |

Extensive (tree) form:

| Function | Concept |
|---|---|
| `ValidExtensiveGame::validate` | tree structure, reachability, information-set partition, limits |
| `to_strategic` | extensive → strategic conversion over complete contingent plans |
| `plan_to_strategy_index` | locate a tree plan in the converted strategic game |
| `solve_backward_induction` | all subgame-perfect equilibria, with a per-node decision log |
| `verify_spe` | confirm or refute a claimed subgame-perfect equilibrium |

## Exact arithmetic

Payoffs arrive as `f64` and are converted to `BigRational` during validation.
Every comparison and every solve is exact. Mixed equilibria come out as exact
fractions — Battle of the Sexes gives 2/3 and 1/3, not 0.6666666666666666.

There is no epsilon and no tolerance parameter, so there is no boundary case
where a strategy is "almost" dominated.

## Honest limits of the answers

- `solve_mixed_nash` enumerates equal-size support pairs, which finds every
  equilibrium of a **non-degenerate** game. Degenerate games can also have
  equilibria on unequal-size supports; those are not found, and the result
  carries `degenerate: true` with a warning rather than presenting a possibly
  partial list as exhaustive.
- `solve_dominance` checks dominance by mixed strategies for **strict**
  dominance on **cardinal** games only. Weak dominance by mixtures is not
  defined in this version, and expected utility over ordinal ranks is
  meaningless, so both cases fall back to pure dominance.
  `mixed_dominance_checked` on the result says which happened; an eliminated
  strategy carries `Dominator::Pure` or `Dominator::Mixed` naming exactly what
  beat it.
- Above 4,096 surviving opponent profiles the mixed-dominance LP is skipped —
  one constraint per profile makes the program impractically large. The result
  is then pure dominance only, with `mixed_dominance_checked: false`. It never
  pretends otherwise.
- An exact LP now exists, but it does **not** close the degenerate mixed-Nash
  gap above. Enumerating unequal-size supports is a separate piece of work;
  `solve_mixed_nash` still reports `degenerate: true` and warns.
- `analyze_repeated_game` needs a pure-strategy Nash equilibrium of the stage
  game to revert to, and returns `GtError::NoPureNashForPunishment` when there
  is none — it does not fall back to a minmax threat, which is generally not
  credible and so not subgame perfect. When a player's punishment payoff is no
  worse than the target's, no discount factor below 1 sustains the profile and
  `critical_discount_factor` is `None` with a note, rather than a threshold no
  legal δ could reach.
- Iterated deletion of *weakly* dominated strategies is order dependent. The
  result is one valid reduction, flagged `order_dependent: true`, not the
  reduction.
- The tree solvers cover **perfect information only**. `to_strategic`,
  `solve_backward_induction`, and `verify_spe` return
  `GtError::ImperfectInformationUnsupported`, naming the offending set, if any
  information set holds more than one node. The schema carries
  `information_sets` so it does not break when imperfect-information solving
  lands, but nothing solves them today.
- Conversion runs one way. There is no strategic → extensive direction; the
  strategic form does not determine a tree.

## Size limits

8 players, 20 strategies per player, 100,000 profiles, 12 strategies per player
for mixed Nash, 10,000 tree nodes. Exceeding one returns
`GtError::GameTooLarge` naming the limit and the actual value. The crate never
truncates a game and answers anyway.

One bound is not an error: past 4,096 surviving opponent profiles the
mixed-dominance LP is skipped, because the whole solve still has a correct
(if weaker) pure-dominance answer to return. That is reported on the result as
`mixed_dominance_checked: false`, not raised.

The strategy limits bind the *converted* game too: a tree where one player owns
many decision nodes has a strategy count that is the product of their action
counts, so `to_strategic` can return `GameTooLarge` for a tree that validated
fine on its own.

## Testing

Unit tests cover named games with known answers. `tests/properties.rs` checks
definitional properties over randomly generated games — reported equilibria
survive independent verification, mixed-equilibrium supports are
payoff-equivalent with nothing outside earning more, mixtures sum to exactly
one, the Pareto frontier is undominated, and no equilibrium pays a player below
their maxmin value.

Four of those properties guard the LP-backed work: pure dominance implies mixed
dominance (the LP must never miss what the cheap check finds), any mixture the
LP reports is re-checked against every opponent profile, every equilibrium
`solve_mixed_nash` produces must pass `verify_mixed_nash`, and the critical
discount factor must behave as a threshold — sustainable at δ\*, not
sustainable below it. That last property is what caught the case where the
punishment ties the target and the formula yields δ\* = 1.

For the tree solvers the load-bearing property is the cross-check from Bonanno
§2.4: every backward-induction solution, read as a profile of complete
contingent plans, must be a Nash equilibrium of the converted strategic form.
That is what catches a `to_strategic` that enumerates only on-path actions —
the single most common way to get dynamic games wrong.

`tests/extensive_fixtures.rs` checks the tree solvers against published
textbook answers. Each fixture in `tests/fixtures/extensive/` carries a game,
its solution, and the page of the published answer; see
`tests/fixtures/README.md`. Dropping a new `.json` there adds a test case.

## Not in this crate

Mixed equilibria of degenerate games on unequal-size supports; `tit_for_tat` as
a punishment strategy; n-player mixed Nash. Imperfect-information solving and
incomplete information — types, beliefs, perfect Bayesian equilibrium,
separating versus pooling — are a later phase.
`docs/reference/product-spec-source.md` §7 records which capabilities were
deferred and why.

## References

Solution concepts follow Giacomo Bonanno, *Game Theory: An open access
textbook with 165 solved exercises*, UC Davis, 2015 —
<http://www.econ.ucdavis.edu/faculty/bonanno/>. Chapter and page anchors for
each function are in `docs/reference/bonanno-concept-map.md`.

Repeated games follow Martin J. Osborne and Ariel Rubinstein, *A Course in Game
Theory*, MIT Press, 1994, ch. 8, rather than Bonanno — that book has no
repeated-games chapter. `analyze_repeated_game` uses discounted-sum payoffs
with δ ∈ [0, 1) and cites no Bonanno chapter anywhere.
