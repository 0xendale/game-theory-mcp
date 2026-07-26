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
| `gt-mcp` | MCP adapter over `gt-core`. Contains no arithmetic. Not yet written. |

## Status

Pre-release. `gt-core` handles strategic-form (simultaneous-move) games:

- validation, with size limits and every problem reported at once
- iterated deletion of dominated strategies
- all pure-strategy Nash equilibria, any number of players
- exact mixed-strategy Nash equilibria for two players
- equilibrium verification — confirms a claimed equilibrium or returns the
  deviation that refutes it
- Pareto frontier, constant-sum detection, security levels, welfare analysis
- classification against named archetypes: prisoner's dilemma, stag hunt,
  chicken, battle of the sexes, matching pennies

Next: extensive-form (sequential) games, backward induction, and repeated games.
Then the MCP server itself. Mechanism design — second-price auctions, VCG — and
games of incomplete information come after that.

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
cargo test --workspace     # 73 unit + 8 property + 1 doc test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

Minimum supported Rust version is 1.85, enforced in CI. `Cargo.lock` is
committed, so dependency changes appear as an explicit diff under review.

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
