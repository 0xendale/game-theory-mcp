# Demo 2 — "Should we believe the price-war threat?" (market entry)

A startup product strategist deciding whether to enter an incumbent's market,
answered with the game-theory MCP tools. The server answers shown are exact and
reproducible — the same tool calls return them every time.

## The person

Product strategy lead at a startup about to launch into a market owned by one
big incumbent, whose CEO just made a very public threat.

## The question

> We're deciding whether to launch. Their CEO said: "If they enter, we will
> fight a price war until they bleed out."
>
> Annual profit in $M: if we stay out, we get 0 and they keep 2. If we enter
> and they actually fight, both sides lose 1. If we enter and they share the
> market instead, we each make 1.
>
> Should we take the threat seriously? What do you expect each side to do if
> we launch?

## What the model did

The model formalized the sequence — we move first (In/Out), then the
incumbent responds (Fight/Accommodate) — and ran it through the server:

| # | Tool | Server returned |
|---|---|---|
| 1 | `validate_game` | tree accepted: 2 players, 5 nodes, singleton information sets |
| 2 | `solve_backward_induction` | unique subgame-perfect equilibrium: **`In` then `Accommodate`**, terminal payoff `(1, 1)`; the `Fight` branch is pruned |

The decision log that comes back names the exact step that kills the threat:
at the incumbent's node, Accommodate pays `1` and Fight pays `-1`. Rational
play never fights. The threat lives only at a node the incumbent never
reaches.

## The plain-English answer the model delivered

- The threat is **not credible**. Once we're in, the incumbent loses money by
  fighting, so they won't — regardless of what their CEO said publicly.
- Backward induction from the end of the game is what makes that visible:
  first predict what the incumbent does *after* we enter, then decide whether
  entering is worth it. Entering earns `1`; staying out earns `0`.
- Recommendation: launch. Budget for a slow accommodation period, not a price
  war.

This is a case where the reasoning *method* is the deliverable. A bare model
may or may not apply backward induction; the MCP server makes the
subgame-perfect logic explicit, step by step, with the pruned branch shown.

## Reproduce

```sh
cd benchmarks
.venv/bin/python -m harness.runner --scenarios market-entry-threat --tag demo
.venv/bin/python -m harness.score --tag demo
```
