# Test fixtures

Each fixture is one JSON file: a game, its answer, and — where the game comes
from published work — the page of its published solution. Fixtures exist so a
solver can be checked against a known-correct answer rather than against itself.

Per the repository rules, a fixture carries **only** game data, the answer, and
a citation by page number — never the source text. Games marked
`"origin": "original"` were constructed for this repository and their answers
derived here. Games citing a page encode a payoff structure whose solution the
cited work publishes; the payoff numbers are data, not prose.

Reference for the cited work:

> Giacomo Bonanno, *Game Theory: An open access textbook with 165 solved
> exercises*, UC Davis, 2015 —
> <http://www.econ.ucdavis.edu/faculty/bonanno/>

## Extensive-form schema

Files live in `extensive/` and are picked up automatically by
`tests/extensive_fixtures.rs` — adding a `.json` file there adds a test case,
no code change needed.

```json
{
  "name": "entry_deterrence",
  "origin": "original",
  "citation": "Constructed for game-theory-core; canonical entry-deterrence structure.",
  "game": { "...": "an ExtensiveGame, exactly as it deserializes" },
  "expected_spe": {
    "paths": [[[0, 0], [1, 1]]],
    "payoffs": [[1, 1]]
  }
}
```

- `game` is an `ExtensiveGame`. Each node carries a `"kind"` tag, `"decision"`
  or `"terminal"`.
- `expected_spe.paths[i]` is the induced path of the i-th subgame-perfect
  equilibrium as `[node, action]` steps, root to terminal.
- `expected_spe.payoffs[i]` is that equilibrium's terminal payoff vector.
- The two lists are parallel and compared as a set, so the order of equilibria
  within a fixture is not significant.

A fixture that fails means either the transcription or the solver is wrong.
Investigate — never edit the expected answer to match the solver.
