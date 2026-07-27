# Product Spec (source): Game Theory Decision Co-Pilot

**Source:** `Game_Theory_MCP_Server_Specification_EN.pdf` — v1.0 (July 2026), 3 pages.
**Status:** verbatim transcription in §1–§6, followed by a review section (§7) recording where this document is superseded by the design that was settled on 2026-07-23.

This file exists so the original product intent stays readable and greppable without opening the PDF. Where the design departs from this spec, §7 says so explicitly — do not treat §1–§6 as current truth on their own.

---

## 1. Executive summary & core goal

The Game Theory Decision Co-Pilot MCP Server is engineered to serve as a **"Formal Game Theory Thinking Co-Pilot"** for Large Language Models such as Claude and ChatGPT.

**Core problem solved:** Standard LLMs typically respond to strategic, marketing, or game design queries with generic advice compiled from general web articles. This MCP Server anchors AI reasoning strictly within mathematical game-theoretic principles and formal strategic interaction frameworks (e.g. Giacomo Bonanno's *Game Theory* textbook).

Header metadata from the PDF:

| Field | Value |
|---|---|
| Architecture | Rust (`rmcp` SDK) |
| Protocol | JSON-RPC over stdio |
| Document version | v1.0 (July 2026) |

## 2. Target audiences & key capabilities

| Target user | Need / pain point | MCP solution provided |
|---|---|---|
| Game designers | Designing game loops, anti-exploit mechanics, and balanced game economies without overpowered (OP) features. | Simulates player incentives, identifies behavioral imbalances, detects exploit/cheating vectors. |
| Marketers | Designing promotion campaigns and gamification without abuse; establishing brand quality trust. | Analyzes separating vs. pooling equilibria; mitigates price war / prisoner's dilemma risks. |
| Business strategists | Predicting competitor reactions during new product entry or strategic price adjustments. | Models dynamic games, subgame-perfect equilibrium (SPE), models retaliation scenarios. |
| Devs & data analysts | Designing distributed systems, auction mechanisms, incentive-aligned user flows. | Provides second-price (Vickrey) auction models, VCG mechanisms, quantitative parameter optimization. |

## 3. System architecture & technical choices

Implemented in **Rust** using the official `rmcp` SDK for performance, memory safety, and thread safety:

- **Minimal footprint:** sub-millisecond startup (<1 ms), ~2–5 MB RAM when running continuously on local machines.
- **Stateless & deterministic execution:** all mathematical computation (matrix games, equilibrium algorithms) executes as pure functions, guaranteeing 100% deterministic output without external API dependencies.
- **Standard protocol interface:** communicates over `stdio` with Claude Desktop and other MCP-compliant host clients.

## 4. Core MCP tools specification (as originally written)

### Tool 1: `formalize_scenario`
Translates natural language user prompts into a formal Game Frame tuple *(I, (S₁, …, Sₙ), Payoffs)*.

### Tool 2: `analyze_mechanic_risks`
Analyzes user-proposed scenarios to identify behavioral vulnerabilities:
- Detects risks of falling into a **prisoner's dilemma** where both players suffer losses.
- Identifies vulnerabilities to sybil attacks, collusion, or exploit farming in incentive schemes.

### Tool 3: `simulate_behavioral_outcomes`
Executes formal mathematical algorithms to compute strategic equilibria:
- Iterated deletion of strictly dominant strategies (IDSDS).
- Pure strategy Nash equilibrium & 2-player mixed strategy Nash equilibrium.
- Backward induction for dynamic games with perfect information.

### Tool 4: `suggest_optimal_parameters`
Provides constructive counter-arguments to the user's scenario and outputs **suggested balancing parameters** (e.g. optimal reward rate *p*, minimum penalty threshold *c*) to achieve equilibrium.

## 5. Implementation blueprint (Rust example, as printed in the PDF)

```rust
// Cargo.toml
// [dependencies]
// rmcp = "0.1"
// tokio = { version = "1.0", features = ["full"] }
// serde = { version = "1.0", features = ["derive"] }
// serde_json = "1.0"

// main.rs (excerpt)
use rmcp::{tool, Server, ToolResult};

#[tool(name = "suggest_optimal_parameters", description = "Suggests optimal balancing parameters for a scenario")]
async fn suggest_optimal_parameters(scenario_json: String) -> ToolResult {
    // Math balancing logic executed in Rust
    let recommendation = "RECOMMENDED_PARAMETERS: reward_rate=0.15, penalty_threshold=500";
    Ok(recommendation.into())
}

#[tokio::main]
async fn main() -> Result<(), Box> {
    let server = Server::builder()
        .name("Game Theory Decision Co-Pilot")
        .version("1.0.0")
        .register_tool(suggest_optimal_parameters)
        .build();

    server.listen_stdio().await?;
    Ok(())
}
```

> The blueprint above is illustrative pseudocode from the PDF, not a compiling program. `Result<(), Box>` is not valid Rust (missing `dyn Error`), and the actual `rmcp` API surface must be confirmed against the published crate before any of it is copied.

## 6. Roadmap & future scope (as originally written)

1. **Phase 1 (v1.0):** initialize Rust project with `rmcp`, implementing the 4 core analysis tools + guided prompts.
2. **Phase 2 (v1.1):** integrate mechanism design templates (second-price / Vickrey auction and VCG mechanisms).
3. **Phase 3 (v1.2):** package as an npm wrapper (for `npx` launcher support) for easy distribution.

---

## 7. Review: where this spec is superseded

Recorded 2026-07-23 during design. Each item below is a deliberate departure agreed with the project owner.

### 7.1 `formalize_scenario` contradicts the determinism guarantee

§3 promises "100% deterministic outputs without external API dependencies". §4 Tool 1 asks the server to parse natural language, which requires an LLM. Both cannot hold.

**Resolution:** the server never parses natural language. It publishes a strict Game JSON schema, a `validate_game` tool that normalizes and diagnoses caller-supplied games, and MCP *prompts* that guide the host LLM through formalization. The host LLM does the reading; the server does the math. `formalize_scenario` as a parsing tool is dropped.

### 7.2 `simulate_behavioral_outcomes` is misnamed and over-bundled

No simulation occurs — these are exact equilibrium algorithms. Bundling three unrelated algorithms behind one tool forces a union return type and makes tool selection ambiguous for the calling model.

**Resolution:** split into `solve_dominance`, `solve_pure_nash`, `solve_mixed_nash`, `solve_backward_induction`, each with its own typed result and reasoning trace.

### 7.3 `analyze_mechanic_risks` is not defined precisely enough to implement or test

"Detects prisoner's dilemma risk" and "identifies sybil / collusion / exploit-farming vulnerabilities" have no stated decision procedure.

**Resolution:** two concrete, testable replacements.
- `analyze_payoff_structure` (v1.0) classifies a game against known archetypes using formal criteria — prisoner's dilemma is *a dominant-strategy equilibrium that is Pareto-dominated by another outcome*. Also reports the Pareto frontier, zero-sum detection, maxmin/security levels, and social welfare.
- `analyze_coalition_stability` (v1.1) formalizes collusion as coalition-deviation profitability (is the profile coalition-proof), with an explicit player-count cap because the coalition lattice is exponential.

### 7.4 `suggest_optimal_parameters` has no defined semantics

The blueprint returns a hardcoded string. As written the tool would be the single largest hallucination surface in the product.

**Resolution:** deferred to phase 2 with real semantics — payoffs declared as expressions over named parameters, and the tool solves for parameter ranges under which a target equilibrium property holds (e.g. "cooperate is strictly dominant"). Reported as intervals with the property restated, never as a bare recommended number.

### 7.5 Separating/pooling equilibria are promised but unimplementable in v1

§2 sells separating vs. pooling equilibrium analysis to marketers. Those are solution concepts for games of incomplete information (Bonanno ch. 10–15) requiring types, priors, beliefs, and Bayesian updating. No tool in §4 supports any of that.

**Resolution:** deferred to phase 2 and **removed from v1 marketing copy**. The v1 schema reserves the shape for information sets so the representation does not break when it lands, but v1 solvers reject non-singleton information sets with a typed error.

### 7.5a Form conversion is one-directional, and tree solving is perfect-information only

The design describes `convert_form` as "extensive ↔ strategic". As implemented it converts **extensive → strategic only**, and every tree solver (`to_strategic`, `solve_backward_induction`, `verify_spe`) requires perfect information, returning `ImperfectInformationUnsupported` and naming the offending information set otherwise.

**Reason, two parts:**

1. *No reverse direction.* A strategic form does not determine a tree — many extensive games share one normal form, differing in who moves when and who observes what. Synthesising "a" tree would mean inventing timing the caller never specified, and any answer about a dynamic property of that invented tree would be an artefact of the invention. There is no correct output to return, so the direction is not offered.
2. *No imperfect information.* Solving non-singleton information sets needs sequential-rationality machinery with beliefs (Bonanno ch. 10–12), which is phase 2 by §7.5 above. Converting under information sets is well-defined but would produce a strategic form no v1.0 tree solver could then reason about, so it lands with the solvers rather than before them.

The `information_sets` field is populated and validated in v1.0 — it must partition the decision nodes, each set confined to one player with equal action counts — so the schema and its checks do not change when solving arrives.

### 7.5b Mixed-equilibrium verification is its own function, and repeated games are grim-trigger only

Three departures from the design, all settled 2026-07-27 while implementing the mixed-strategy LP increment.

1. *`mixed_nash` is not a `Concept` variant.* The design lists `verify_equilibrium` as taking a concept of `pure_nash | mixed_nash | spe | dominant_strategy`. `verify_equilibrium` takes `&[StrategyId]` — a pure profile — and a mixed profile is a list of probability distributions, not strategy indices. Widening that signature would force every pure-Nash caller to build degenerate mixtures. So mixed verification is `verify_mixed_nash`, a standalone typed function, exactly as `verify_spe` is (§7.5a). Dispatching `concept: mixed_nash` to it is the MCP layer's job, not `gt-core`'s.

2. *`analyze_repeated_game` covers `grim_trigger` only.* The design floats `tit_for_tat` as optional; it is deferred. Tit-for-tat's sustainability conditions need stage-game symmetry assumptions that grim trigger does not, and grim trigger alone answers the price-war and cooperation-dilemma use cases the spec sells. The punishment is reversion to a pure-strategy stage Nash equilibrium rather than the minmax value: minmax is generally not credible, so a minmax-threat δ\* would not be a subgame-perfect answer. A stage game with no pure Nash equilibrium returns `NoPureNashForPunishment` instead of silently switching threat.

   Payoff convention recorded here so it stays citable: **discounted sum**, δ ∈ [0, 1), source Osborne & Rubinstein, *A Course in Game Theory*, ch. 8. This closes the design's open question on repeated-games sourcing; see `bonanno-concept-map.md` §4.

3. *The exact LP does not close the degenerate mixed-Nash gap.* The design notes that completing the degenerate case "needs an exact rational LP solver written from scratch". That LP now exists (`solve_lp`, two-phase simplex over rationals) and is used for mixed dominance, but enumerating equilibria on unequal-size supports is a separate piece of work and was not done. `solve_mixed_nash` still reports `degenerate: true` with a warning. The gap is narrower than it was, not closed.

### 7.6 Additions not present in the source spec

Agreed during design, in phase order.

| Addition | Phase | Rationale |
|---|---|---|
| `validate_game` | 1.0 | Nothing else is meaningful without a canonical, checked representation. |
| `verify_equilibrium` | 1.0 | Caller claims a profile is an equilibrium; server confirms or returns the profitable deviation. Directly counters LLM-invented equilibria. |
| Archetype classification | 1.0 | Makes the §4 Tool 2 prisoner's-dilemma claim mathematically real. |
| `analyze_repeated_game` | 1.0 | Grim-trigger discount-factor threshold for sustaining cooperation. The actual answer to the price-war and cooperation-dilemma use cases §2 sells. Note: **not covered by Bonanno** — see `bonanno-concept-map.md` §4. |
| `analyze_coalition_stability` | 1.1 | Replaces the vague collusion bullet. |
| Typed error model | 1.0 | Named failures (`ImperfectInformationUnsupported`, `OrdinalPayoffsRejected`, `GameTooLarge`) rather than stringly errors. |
| MCP resources + prompts | 1.0 | Ships concept references anchored to Bonanno chapters; makes the "textbook-anchored" claim real. |

### 7.7 Roadmap restated

The source roadmap is kept in shape but repopulated:

- **v1.0** — foundation, static solvers, dynamic solver, verification, structure analysis, repeated games.
- **v1.1** — mechanism design templates (second-price/Vickrey, pivotal/VCG — Bonanno §1.3–1.4) + coalition stability.
- **v1.2** — parametric solving; incomplete information (types, beliefs, PBE, separating/pooling).
- **v1.3** — npm wrapper for `npx` distribution.
