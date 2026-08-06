# game-theory-mcp

[![crates.io](https://img.shields.io/crates/v/game-theory-mcp.svg)](https://crates.io/crates/game-theory-mcp)
[![docs.rs](https://docs.rs/game-theory-core/badge.svg)](https://docs.rs/game-theory-core)
[![license](https://img.shields.io/crates/l/game-theory-mcp.svg)](LICENSE)

An MCP server that gives an LLM exact game-theoretic computation — equilibrium
solving, dominance analysis, and structural classification of strategic
interactions, computed rather than estimated.

Written in Rust. No network calls, no API keys, no stored state.

## Why

Ask a language model whether a pricing scheme invites a race to the bottom, or
whether a game economy rewards an exploit, and you usually get plausible
strategic advice assembled from prose. Sometimes it is right. There is no way to
tell from the answer.

Game theory has exact answers to a lot of those questions. This server computes
them, so the model reasons from a derivation instead of a recollection.

## How the work is split

The server never reads natural language, and the model never does the
arithmetic.

The model formalizes the scenario into a Game JSON object — who the players are,
what they can do, what each outcome is worth to each of them. The server checks
that object and returns exact answers with the reasoning that produced them. So
the parts that need judgment stay with the model, and the parts that need to be
right stay in Rust.

That division is also what keeps the determinism claim true: same game in, same
answer out, every time.

## Structure

| Crate | Role |
|---|---|
| [`game-theory-core`](crates/game-theory-core) ([crates.io](https://crates.io/crates/game-theory-core)) | All mathematics. No MCP, async, or I/O — every algorithm is testable without protocol machinery, and the crate is usable on its own as a library. |
| [`game-theory-mcp`](crates/game-theory-mcp) ([crates.io](https://crates.io/crates/game-theory-mcp)) | MCP adapter over `game-theory-core`. Tool registration, wire types, error mapping; contains no arithmetic. |

## Status

v0.1.0, published. The v1.0 surface is complete: every solver
`game-theory-core` implements is exposed as an MCP tool, alongside the concept
resources and formalization prompts.

`game-theory-core` handles strategic-form (simultaneous-move) games:

- validation, with size limits and every problem reported at once
- iterated deletion of dominated strategies, including dominance by *mixed*
  strategies via an exact rational LP
- all pure-strategy Nash equilibria, any number of players
- exact mixed-strategy Nash equilibria for two players
- equilibrium verification — confirms a claimed equilibrium or returns the
  deviation that refutes it
- Pareto frontier, constant-sum detection, security levels, welfare analysis
- classification against named archetypes: prisoner's dilemma, stag hunt,
  chicken, battle of the sexes, matching pennies

and extensive-form (sequential) games:

- tree validation — reachability, acyclicity, information-set partitioning
- backward induction returning every subgame-perfect equilibrium, with a
  per-node decision log
- subgame-perfection verification, which catches non-credible threats sitting
  at unreached nodes
- conversion to strategic form, handling the plans-at-unreachable-nodes rule
  that makes the conversion easy to get wrong

and infinitely repeated games: grim trigger with Nash reversion, returning the
exact critical discount factor above which a target profile is sustainable.

`game-theory-mcp` serves all nine tools over stdio, plus six `gt://concepts/*`
resources and three formalization prompts. Mechanism design — second-price
auctions, VCG — and games of incomplete information come next.

## Running the server

```sh
cargo install game-theory-mcp
```

Point an MCP client at the installed binary:

```json
{ "mcpServers": { "game-theory": { "command": "game-theory-mcp" } } }
```

To run from a checkout instead, `cargo build --release -p game-theory-mcp` and
point the client at `target/release/game-theory-mcp`.

### Tools

| Tool | What it does |
|---|---|
| `validate_game` | Normalizes and checks a game in matrix, strategic, or extensive form, reporting every problem it finds rather than the first. |
| `convert_form` | Extensive → strategic, returning the plan each generated strategy stands for — the mapping without which the converted matrix cannot be read. |
| `solve_dominance` | Iterated deletion, `strict`, `weak`, or `both`, with the per-round elimination log. Weak results carry an order-dependence warning. |
| `solve_pure_nash` | Every pure-strategy equilibrium, any player count, plus the table the answer was read off. |
| `solve_mixed_nash` | Two-player exact mixed equilibria as fractions, flagging degeneracy rather than presenting a possibly-partial list as exhaustive. |
| `solve_backward_induction` | Every subgame-perfect solution of a perfect-information tree, with a per-node decision log. Ties return all solutions, never one picked silently. |
| `verify_equilibrium` | Checks a claimed equilibrium under `pure_nash`, `dominant_strategy`, `mixed_nash`, or `spe`, returning the profitable deviation when the claim fails. |
| `analyze_payoff_structure` | Pareto frontier, constant-sum detection, security levels, welfare gap, and archetype classification reported as the criteria matched, not a bare label. |
| `analyze_repeated_game` | The exact critical discount factor above which a target profile survives under grim trigger. |

A game with no equilibrium under the requested concept returns an empty list,
not an error. A malformed game comes back as a tool result carrying diagnostics
and a suggested fix, not as a protocol error — the model is the one that has to
correct it, so it needs to be able to read it.

### Resources and prompts

Six concept resources at `gt://concepts/{nash, dominance, subgame-perfect,
mixed-strategies, archetypes, repeated-games}` explain each concept in the terms
the tools use and name the tool that computes it, with textbook citations.

Three prompts — `formalize_scenario`, `analyze_competitive_dynamic`,
`design_incentive_scheme` — guide the host model through turning a situation
into a game: picking players, enumerating strategies, deciding whether payoffs
are ordinal or cardinal. The server never parses prose; it does the arithmetic
once the model has formalized the problem.

## Exact arithmetic

Payoffs arrive as `f64` and are converted to exact rationals during validation.
Every comparison and every solve is exact.

There is no epsilon and no tolerance parameter, so no strategy is ever "almost"
dominated and no player "almost" indifferent. Mixed equilibria are reported as
fractions: Battle of the Sexes gives 2/3 and 1/3, not 0.6666666666666666. This
is what makes agreement with published textbook answers a real test rather than
an approximate one.

## Honest limits

Answers state their own scope rather than overreaching:

- Mixed-strategy solving is exhaustive for non-degenerate two-player games.
  Degenerate games can hide equilibria the enumeration does not reach; those
  results are flagged rather than presented as complete.
- Iterated deletion of *weakly* dominated strategies depends on elimination
  order. The result is one valid reduction, marked as such — not the reduction.
- Exceeding a size limit is an error naming the limit and the actual value. No
  game is silently truncated and answered anyway.

## Development

```sh
cargo test --workspace     # 304 unit + 16 property + 16 end-to-end + 1 fixture + 3 doc
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

The workspace requires Rust **1.88**, set once in `[workspace.package]`,
inherited by both crates and enforced by a CI leg that checks the whole
workspace on that exact toolchain. The floor comes from `rmcp` 3.x; nothing in
`game-theory-core`'s mathematics needs a compiler that new, but a single number
is one fewer thing to keep in step.

`Cargo.lock` is committed, so dependency changes appear as an explicit diff
under review.

## Documentation

Start at [`docs/README.md`](docs/README.md).

- [`docs/reference/product-spec-source.md`](docs/reference/product-spec-source.md)
  — the original product specification, plus §7, the running record of where the
  delivered design departs from it and why.
- [`docs/reference/bonanno-concept-map.md`](docs/reference/bonanno-concept-map.md)
  — each solver mapped to its textbook chapter and page.

## Reference

Solution concepts follow Giacomo Bonanno, *Game Theory: An open access textbook
with 165 solved exercises*, University of California Davis, 2015, available from
the author: <http://www.econ.ucdavis.edu/faculty/bonanno/>.

Solvers are tested against that book's published exercise answers, cited by page
so anyone with a copy can check the work. Where it has no chapter on a topic —
repeated games, notably — the source used is named instead of implied.

## License

MIT — see [LICENSE](LICENSE).

The Bonanno textbook cited above is a separate third-party work under its own
terms and is not distributed here.
