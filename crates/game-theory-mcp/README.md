# game-theory-mcp

An MCP server that gives an LLM exact game-theoretic computation. Nine tools,
six concept resources, three formalization prompts, over stdio.

All mathematics lives in [`game-theory-core`](https://crates.io/crates/game-theory-core);
this crate is the adapter and contains no arithmetic.

## Install and run

```sh
cargo install game-theory-mcp
```

Point an MCP client at the installed binary:

```json
{ "mcpServers": { "game-theory": { "command": "game-theory-mcp" } } }
```

No network calls, no API keys, no stored state.

## Tools

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

## Resources and prompts

Six concept resources at `gt://concepts/{nash, dominance, subgame-perfect,
mixed-strategies, archetypes, repeated-games}` explain each concept in the terms
the tools use and name the tool that computes it, with textbook citations.

Three prompts — `formalize_scenario`, `analyze_competitive_dynamic`,
`design_incentive_scheme` — guide the host model through turning a situation
into a game. The server never parses prose; the model formalizes, the server
computes.

## What comes back

Every number is an exact rational: `{"exact": "1/3", "approx": 0.333…}`, where
`exact` is authoritative and the decimal is for display only. Input is
asymmetric on purpose — probabilities and discount factors must be written as
fractions, because accepting `0.3333333333333333` for `1/3` would reintroduce
at the protocol boundary the tolerance the solvers do not have.

A game with no equilibrium under the requested concept returns an empty list,
not an error.

A domain failure — an ordinal game sent to a tool that takes expectations, a
game over a size limit, a malformed profile — comes back as a *successful* tool
result carrying `isError: true` and a structured payload with a stable `code`, a
message, and a suggestion. It is not a JSON-RPC error, because the model is the
one that has to fix it and some hosts surface only an error's `message`. Only a
request the model cannot act on at all (unparseable JSON, an unknown tool, a
decimal where an exact fraction is required) is a protocol error.

## References

Solution concepts follow Giacomo Bonanno, *Game Theory: An open access textbook
with 165 solved exercises*, UC Davis, 2015 —
<http://www.econ.ucdavis.edu/faculty/bonanno/>. Solvers are tested against that
book's published exercise answers, cited by page.

Repeated games follow Martin J. Osborne and Ariel Rubinstein, *A Course in Game
Theory*, MIT Press, 1994, ch. 8, because Bonanno has no repeated-games chapter.
The `gt://concepts/repeated-games` resource says so rather than implying an
anchor it does not have.

## License

MIT
