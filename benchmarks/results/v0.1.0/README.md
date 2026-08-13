# Benchmark results — gpt-5-mini vs gpt-5.4-mini (v0.1.0)

Full matrix: 8 persona scenarios x 2 arms (bare, mcp) x 3 trials x 2 models =
**96 cells, 0 protocol errors**. Raw transcripts in `raw/` (gitignored),
aggregates in `summary.json`, table in `summary.md`.

## Why this benchmark exists

The MCP server's whole claim is that an LLM should *compute* strategic answers
rather than *recall* them. This benchmark tests that claim directly: the same
question, asked twice of the same model — once with nothing but its training
weights, once with the nine game-theory tools over stdio. The only difference
between arms is whether the tools exist.

The scenarios are deliberately written for a non-specialist audience (growth
marketing, product strategy, partnerships) in business language, because that
is the actual user the server targets. The answer keys were **computed by the
server itself** (`compute_keys.py`), never hand-derived — an assertion a
solver cannot satisfy unless the mathematics is right.

## Headline

| Model | bare | mcp | delta | mcp tool calls/trial |
|---|---|---|---|---|
| gpt-5-mini | 0.861 | **0.986** | **+0.125** | 3.5 |
| gpt-5.4-mini | 0.820 | **0.847** | +0.028 | 2.9 |

- **The smaller model gains the most.** gpt-5-mini bare misses ~1 in 7 exact
  assertions; with MCP it lands 98.6%. The tool-armed model formalizes the
  scenario, calls the server, and reports the exact fraction — instead of
  guessing the fraction.
- **The stronger model is already close.** gpt-5.4-mini scores 0.82 bare; the
  tools close most of the remaining gap. Its gains concentrate in the
  scenarios where the exact value is the entire answer (budget-standoff:
  0.67 -> 0.78; platform-shift: 0.67 -> 0.67 with 4 tool calls that produced
  the right reasoning but a worded answer that missed a regex).
- **MCP never hurts.** No cell scored *lower* with tools than without. The
  cost is latency (a tool round-trip per call) and tokens, both reported per
  trial in `summary.json`.

## Where MCP shows up

| Scenario | gpt-5-mini bare -> mcp | gpt-5.4-mini bare -> mcp | why |
|---|---|---|---|
| budget-standoff | 0.78 -> **1.00** | 0.67 -> 0.78 | exact 2/3 mixed equilibrium + negative EV; the numbers ARE the answer |
| platform-shift-coordination | 0.78 -> **1.00** | 0.67 -> 0.67 | two pure equilibria + Pareto reasoning; tools nail the Pareto claim |
| perk-pruning-order | 0.67 -> 0.89 | 0.67 -> 0.78 | order-dependence of weak dominance — a subtlety bare models flatten |
| market-entry-threat | 1.00 -> 1.00 | 0.89 -> 0.78 | SPE backward induction; both arms strong, bare already got it |

The consistent pattern: MCP helps most where the answer is an **exact
quantity** (a fraction, an equilibrium set, a credibility verdict) that a
model cannot derive reliably from prose. It helps least where the scenario is
a plain reading exercise both arms can do.

## Method (reproduce)

```sh
cd benchmarks
.venv/bin/python -m harness.runner --provider copilot \
    --models gpt-5-mini gpt-5.4-mini --tag v0.1.0 --concurrency 4
.venv/bin/python -m harness.score --tag v0.1.0 --models gpt-5-mini gpt-5.4-mini
```

- Temperature 0. One fresh MCP session per trial (no cross-trial state).
- Scoring: regex assertions per scenario, matched against the final answer,
  case-insensitive. A trial's score = fraction of assertions matched.
- gpt-5.x run through Copilot's Responses API (the harness switches protocol
  per model; see `harness/agent.py` `RESPONSES_MODELS`).

## Honest caveats

- Assertions are regex sets; a model that *derives* the right number in
  slightly different wording can miss a strict pattern (and score as a miss),
  while a model that parrots the words can hit. Read the raw transcripts
  before trusting a single cell.
- 3 trials per cell. Spread is visible in `summary.json` `per_trial`.
- MCP tool calls are counted, not validated for *correct* use — a trial that
  calls the right tool gets credit even if it then misreports the answer.
