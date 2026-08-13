# Demo 1 — "Can we hold the price?" (pricing war, growth marketing)

A realistic session with a growth marketing lead asking about a pricing war,
answered by deepseek-v4-flash with the game-theory MCP tools attached. This is
the transcript of a real benchmark run — the tool sequence below is exactly
what the model executed, and every server answer is reproduced verbatim.

## The person

Growth lead at a coffee subscription startup, about to face a price war with
one rival. No game-theory background, wants a decision, not a lecture.

## The question

> Every week, we and our main rival each pick one of two prices: hold at the
> current price, or undercut by $2. Weekly profit in $k: if we both hold, we
> each make 8. If one undercuts while the other holds, the undercutter makes 10
> and the holder makes 0. If we both undercut, we each make 2.
>
> This repeats every week, indefinitely. A consultant proposed: both hold, and
> if either side ever undercuts, both switch to undercutting forever.
>
> How patient do both sides need to be for "both hold" to actually hold up
> under that arrangement? I want the exact threshold.

## What the model did

The MCP-armed model did not reason from memory. It turned the story into a
matrix game and asked the server — one round, six tools:

| # | Tool | Server returned |
|---|---|---|
| 1 | `validate_game` | `ok: true`, 2 players, matrix form accepted |
| 2 | `analyze_payoff_structure` | `archetype: prisoners_dilemma` — mutual undercutting is the dominant outcome, Pareto-dominated by holding |
| 3 | `solve_pure_nash` | unique pure equilibrium `(Undercut, Undercut)` |
| 4 | `solve_mixed_nash` | no mixed equilibrium exists beyond the pure one |
| 5 | `solve_dominance` | iterated strict dominance eliminates Holding for both players |
| 6 | `analyze_repeated_game` | **`critical_discount_factor: exact 1/4`** — mutual Hold is sustainable under grim trigger iff the weekly discount factor ≥ 1/4 |

The decisive number is exact: `1/4`, not `0.25` or "somewhere around a
quarter". The model reported it as a fraction because the server computes
exactly.

## The plain-English answer the model delivered

- On its own, this market ends in both sides undercutting and making **2** each
  per week — the "safe" move is also the worst joint outcome.
- If both sides value next week's profit at **at least 1/4 of this week's**,
  the mutual-hold agreement survives: any undercut triggers permanent
  undercutting, so the one-time gain of 10 is not worth losing the recurring 8.
- Below δ = 1/4 the threat isn't scary enough and holding collapses.

A marketing lead gets an exact patience threshold they can put in a
recommendation, plus the reason it holds — not a confident guess about
"competitive dynamics".

## Reproduce

```sh
cd benchmarks
.venv/bin/python -m harness.runner --scenarios price-war-collusion --tag demo
.venv/bin/python -m harness.score --tag demo
```

The tool outputs above are deterministic — the same game in, the same exact
answers out, every time.
