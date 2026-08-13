# Demo 3 — "What rotation do we play?" (promo calendar, partnerships)

Two brands sharing one email list, embargoed from coordinating, needing an
exact randomization for their promo weeks. The server answer shown is exact
and reproducible.

## The person

Partnerships marketing manager at a brand that co-runs a shared email list with
a partner. They cannot talk to the partner this quarter — the problem is
coordination under a communication ban.

## The question

> We and a partner brand share one email list. Each of us independently picks
> a promo week: week A or week B. We can't coordinate — there's an embargo on
> talking to each other this quarter.
>
> Payoffs in qualified leads (thousands): if we both pick week A, we get 2 and
> they get 1. If we both pick week B, we get 1 and they get 2. If we pick
> different weeks, the list gets spam fatigue and we both get 0.
>
> Is there a sensible randomized rotation, and if so, what are the exact
> probabilities each side should use? They want numbers, not vibes.

## What the model did

The model recognized the structure (each side prefers a *different* joint
outcome, nobody wants to mismatch) and asked the server for the mixed
equilibrium:

| # | Tool | Server returned |
|---|---|---|
| 1 | `validate_game` | matrix accepted, 2 players, cardinal payoffs |
| 2 | `solve_mixed_nash` | three equilibria: the two pure ones `(A,A)` and `(B,B)` plus the mixed one |

The mixed equilibrium is the one that matters under the embargo, because
neither side can commit to a pure week. The server returns it as exact
fractions:

- **Us: play Week A with probability 2/3, Week B with probability 1/3.**
- **Partner: play Week A with probability 1/3, Week B with probability 2/3.**

Both sides' expected payoff under this rotation is exactly **2/3** (thousand
leads) — strictly better than the 0 from mismatching, and each side is
indifferent between their two weeks, which is exactly why the mix is stable.

## The plain-English answer the model delivered

- There is no coordination-free pure plan that both sides would stick to —
  each prefers a different week, and a fixed plan is exploitable.
- The stable randomized rotation favors **each side's own preferred week by
  two-to-one**: we go Week A most of the time, they go Week B most of the
  time, and the slight mismatches are the price of not communicating.
- If the embargo lifts, drop the randomness and coordinate on the asymmetric
  split — but until then, 2/3 : 1/3 is the play.

"Randomize" is easy to say; the *exact* probabilities and the expected payoff
are what the server provides — fractions, not `0.666…` that a decimal
conversion would have to round.

## Reproduce

```sh
cd benchmarks
.venv/bin/python -m harness.runner --scenarios promo-calendar-mix --tag demo
.venv/bin/python -m harness.score --tag demo
```