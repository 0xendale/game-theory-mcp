# gt-core

Exact game-theoretic computation. No I/O, no async, no protocol handling.

`gt-mcp` is the MCP adapter over this crate; this crate is usable on its own
as a library.

## What it does

| Function | Concept |
|---|---|
| `ValidStrategicGame::validate` | structural validation, limits, exact payoff table |
| `solve_dominance` | iterated deletion of strictly or weakly dominated strategies |
| `solve_pure_nash` | all pure-strategy Nash equilibria, any player count |
| `solve_mixed_nash` | all mixed equilibria of a non-degenerate 2-player game |
| `verify_equilibrium` | confirm or refute a claimed equilibrium |
| `analyze_structure` | Pareto frontier, constant-sum, security levels, welfare |
| `classify` | prisoner's dilemma, stag hunt, chicken, battle of the sexes, matching pennies |

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
- `solve_dominance` currently checks dominance by *pure* strategies only. A
  strategy can be strictly dominated by a mixture while no pure strategy
  dominates it, so iterated deletion may under-eliminate on cardinal games
  until mixed dominance lands.
- Iterated deletion of *weakly* dominated strategies is order dependent. The
  result is one valid reduction, flagged `order_dependent: true`, not the
  reduction.

## Size limits

8 players, 20 strategies per player, 100,000 profiles, 12 strategies per player
for mixed Nash. Exceeding one returns `GtError::GameTooLarge` naming the limit
and the actual value. The crate never truncates a game and answers anyway.

## Testing

Unit tests cover named games with known answers. `tests/properties.rs` checks
eight definitional properties over randomly generated games — reported
equilibria survive independent verification, mixed-equilibrium supports are
payoff-equivalent with nothing outside earning more, mixtures sum to exactly
one, the Pareto frontier is undominated, and no equilibrium pays a player below
their maxmin value.

## Not in this crate

Extensive-form games, backward induction, repeated games, and dominance by
mixed strategies are the next increment. Incomplete information — types,
beliefs, perfect Bayesian equilibrium, separating versus pooling — is a later
phase. `docs/reference/product-spec-source.md` §7 records which capabilities
were deferred and why.

## References

Solution concepts follow Giacomo Bonanno, *Game Theory: An open access
textbook with 165 solved exercises*, UC Davis, 2015 —
<http://www.econ.ucdavis.edu/faculty/bonanno/>. Chapter and page anchors for
each function are in `docs/reference/bonanno-concept-map.md`.

Repeated games, when they land, follow Osborne & Rubinstein rather than
Bonanno — that book has no repeated-games chapter.
