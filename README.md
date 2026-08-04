# game-theory-mcp

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
| [`gt-core`](crates/gt-core) | All mathematics. No MCP, async, or I/O — every algorithm is testable without protocol machinery. |
| `gt-mcp` | MCP adapter over `gt-core`. Tool registration, wire types, error mapping; contains no arithmetic. |

## Status

Pre-release. The mathematics is largely complete; the server exposes two tools
so far.

`gt-core` handles strategic-form (simultaneous-move) games:

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

`gt-mcp` currently serves `validate_game` and `verify_equilibrium` over stdio.
The remaining solvers, the `gt://concepts/*` resources, and the formalization
prompts follow the pattern those two establish. Mechanism design — second-price
auctions, VCG — and games of incomplete information come after that.

## Running the server

```sh
cargo build --release -p gt-mcp
```

Point an MCP client at the resulting binary:

```json
{ "mcpServers": { "game-theory": { "command": "/path/to/target/release/gt-mcp" } } }
```

Two tools are available. `validate_game` normalizes and checks a game, in
matrix, strategic, or extensive form, reporting every problem it finds rather
than the first. `verify_equilibrium` checks a claimed equilibrium under
`pure_nash`, `dominant_strategy`, `mixed_nash`, or `spe`, and returns the
profitable deviation when the claim does not hold.

A malformed game comes back as a tool result carrying diagnostics and a
suggested fix, not as a protocol error — the model is the one that has to
correct it, so it needs to be able to read it.

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
cargo test --workspace     # 211 unit + 16 property + 4 end-to-end + 1 fixture + 3 doc
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

The two crates have different minimum Rust versions, each enforced by its own
CI leg. `gt-core` requires **1.85**; `gt-mcp` requires **1.88**, because `rmcp`
3.x does. Keeping them separate costs one CI job and preserves `gt-core`'s
portability — it is the reusable half, and nothing in its mathematics needs the
newer compiler. Building the workspace therefore needs 1.88 or later.

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

MIT OR Apache-2.0
