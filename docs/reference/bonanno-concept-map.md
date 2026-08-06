# Bonanno concept map — textbook → tool → phase

**Source:** Giacomo Bonanno, *Game Theory: An open access textbook with 165 solved exercises*, University of California Davis, 2015. Freely available from the author: <http://www.econ.ucdavis.edu/faculty/bonanno/>. Licensed CC BY-NC-ND 4.0 — cite and reference by page; do not reproduce or redistribute the text.

Page numbers below refer to the book's own printed numbering, so any copy obtained from the author's page can be followed along.

Purpose of this file: give every planned tool a specific textbook anchor (chapter, section, page), and mark honestly which parts of the product are **not** covered by this book. Page numbers are the book's own numbering as printed in its table of contents.

Scope legend:
- **v1.0** — first shippable server
- **v1.1** — mechanism design + coalition stability
- **v1.2** — parametric solving + incomplete information
- **out** — deliberately not built

---

## 1. Part I — Games with ordinal payoffs (p. 5)

### Ch. 1 Ordinal games in strategic form (p. 6)

| § | Concept | p. | Maps to | Phase |
|---|---|---|---|---|
| 1.1 | Game frames and games; outcomes vs. preferences | 6 | `StrategicGame` schema; the frame/game distinction is why `payoff_kind` exists | v1.0 |
| 1.2 | Strict and weak dominance | 14 | `solve_dominance` | v1.0 |
| 1.3 | Second-price auction | 21 | `design_second_price_auction` | v1.1 |
| 1.4 | The pivotal mechanism | 24 | `design_pivotal_mechanism` (VCG) | v1.1 |
| 1.5 | Iterated deletion procedures | 28 | `solve_dominance` — IDSDS and IEWDS, with the order-dependence warning for weak deletion | v1.0 |
| 1.6 | Nash equilibrium | 32 | `solve_pure_nash`, `verify_equilibrium` | v1.0 |
| 1.7 | Games with infinite strategy sets | 37 | **out** — schema is finite-strategy only; `validate_game` rejects unbounded sets | out |

Appendix 1.A proofs (p. 40) · 1.E 23 exercises (p. 43) · 1.S solutions (p. 51).

**Key definition to encode verbatim:** §1.1 separates a *game frame* (players, strategies, outcomes) from a *game* (frame + preference relation over outcomes). The `analyze_payoff_structure` archetype classifier must operate on the game, not the frame — the same frame with different preferences is a different game. The Split-or-Steal example in §1.1 is the canonical demonstration and belongs in the test corpus.

### Ch. 2 Dynamic games with perfect information (p. 65)

| § | Concept | p. | Maps to | Phase |
|---|---|---|---|---|
| 2.1 | Trees, frames and games | 65 | `ExtensiveGame` schema | v1.0 |
| 2.2 | Backward induction | 69 | `solve_backward_induction` | v1.0 |
| 2.3 | Strategies in perfect-information games | 73 | `convert_form` — a strategy specifies an action at *every* decision node of that player, including unreached ones | v1.0 |
| 2.4 | Relationship between backward induction and other solutions | 76 | Cross-checks in the test suite: every BI solution is a Nash equilibrium of the converted strategic form | v1.0 |
| 2.5 | Perfect-information games with two players | 81 | Zermelo-style results; test corpus | v1.0 |

Appendix 2.E 13 exercises (p. 84) · 2.S solutions (p. 92).

**Trap to encode:** §2.3 is the most common source of wrong answers. A strategy is a *complete contingent plan*, not a path. `convert_form` must enumerate actions at unreached nodes or the resulting strategic form will be wrong and the ch. 2.4 cross-check test will catch it.

### Ch. 3 General dynamic games (p. 107)

| § | Concept | p. | Maps to | Phase |
|---|---|---|---|---|
| 3.1 | Imperfect information (information sets) | 107 | Schema reserves `information_sets` in v1.0; solvers reject non-singletons with `ImperfectInformationUnsupported` | schema v1.0, solving v1.2 |
| 3.2 | Strategies | 114 | `convert_form` under information sets | v1.2 |
| 3.3 | Subgames | 116 | Subgame decomposition | v1.2 |
| 3.4 | Subgame-perfect equilibrium | 119 | `solve_subgame_perfect` (general case). v1.0 covers only the perfect-information special case via backward induction | v1.2 |
| 3.5 | Games with chance moves | 127 | Chance nodes require cardinal payoffs and expected-value folding | v1.2 |

Appendix 3.E 14 exercises (p. 132) · 3.S solutions (p. 139).

---

## 2. Part II — Games with cardinal payoffs (p. 157)

### Ch. 4 Expected utility (p. 158)

| § | Concept | p. | Maps to | Phase |
|---|---|---|---|---|
| 4.1 | Money lotteries and attitudes to risk | 158 | Documentation only — explains why `payoff_kind: cardinal` is a substantive claim about the caller's utilities, not a formatting choice | v1.0 (as an MCP resource) |
| 4.2–4.3 | Expected utility theorems and axioms | 160, 169 | Justifies refusing mixed-strategy computation on ordinal payoffs | v1.0 |

Appendix 4.E 14 exercises (p. 177) · 4.S solutions (p. 181).

**This chapter is the reason `payoff_kind` exists.** Ordinal payoffs are ranks; expected values over ranks are meaningless. Any tool that averages payoffs (`solve_mixed_nash`, `analyze_repeated_game`, chance nodes) must return `OrdinalPayoffsRejected` rather than a number.

### Ch. 5 Mixed strategies in strategic-form games (p. 186)

| § | Concept | p. | Maps to | Phase |
|---|---|---|---|---|
| 5.1 | Strategic-form games with cardinal payoffs | 186 | `payoff_kind: cardinal` path | v1.0 |
| 5.2 | Mixed strategies | 190 | `MixedStrategy` type — probability vector over own strategies | v1.0 |
| 5.3 | Computing the mixed-strategy Nash equilibria | 195 | `solve_mixed_nash` — 2-player support enumeration; the indifference conditions in this section are the correctness spec | v1.0 |
| 5.4 | Strict dominance and rationalizability | 201 | Dominance by *mixed* strategies — strictly stronger than pure dominance, and a real requirement on `solve_dominance` | v1.0 |

Appendix 5.E 15 exercises (p. 205) · 5.S solutions (p. 213).

**Requirement easy to miss:** §5.4 — a pure strategy can be strictly dominated by a mixed strategy while being dominated by no pure strategy. `solve_dominance` on cardinal games must check mixed dominance (an LP feasibility check per candidate strategy), not just pairwise pure comparison. On ordinal games only pure dominance is defined.

### Ch. 6 Dynamic games with cardinal payoffs (p. 224)

| § | Concept | p. | Maps to | Phase |
|---|---|---|---|---|
| 6.1 | Behavioral strategies | 224 | v1.2 | v1.2 |
| 6.2 | Subgame-perfect equilibrium revisited | 231 | v1.2 | v1.2 |
| 6.3 | Problems with subgame-perfect equilibrium | 235 | Motivates Part IV refinements | v1.2 |

Appendix 6.E 9 exercises (p. 237) · 6.S solutions (p. 243).

---

## 3. Parts III–V — advanced topics

All of Part III–V is **out of v1.0 and v1.1**. Recorded here so the phase-2 spec has its anchors ready.

### Part III — Knowledge, common knowledge, belief (p. 252)

| Ch. | Topic | p. | Relevance |
|---|---|---|---|
| 7 | Knowledge and common knowledge (individual 253, interactive 257, common 265) | 253 | Foundation for Part V's state-space treatment of incomplete information. Not directly a tool. |
| 8 | Adding beliefs to knowledge — probabilistic beliefs 289, Bayes' rule 292, belief revision 296, Harsanyi consistency 301, agreeing to disagree 306 | 289 | §8.2 Bayesian updating is the machinery behind every phase-2 belief-based solver. |
| 9 | Common knowledge of rationality — strategic form 336, extensive form 339 | 332 | Theoretical justification for IDSDS. Worth citing in the `solve_dominance` explanation text; not a tool. |

### Part IV — Refinements of subgame-perfect equilibrium (p. 353)

| Ch. | Topic | p. | Phase |
|---|---|---|---|
| 10 | Weak sequential equilibrium — assessments and sequential rationality 354, Bayesian updating at reached information sets 361 | 353 | v1.2 |
| 11 | Sequential equilibrium — consistent assessments 389 | 389 | v1.2 or later; §11.3 notes consistency is hard to verify in practice, which makes this a poor tool target |
| 12 | Perfect Bayesian equilibrium — AGM consistency 418, Bayesian consistency 423, PBE 426 | 418 | v1.2 — **preferred phase-2 target over ch. 11**, since Bonanno §12.5 gives a characterization free of topological conditions |

### Part V — Incomplete information (p. 470)

| Ch. | Topic | p. | Phase |
|---|---|---|---|
| 13 | Static games — one-sided 473, two-sided 482, multi-sided 486 | 471 | v1.2 |
| 14 | Dynamic games — one-sided 510, multi-sided 528 | 510 | v1.2 — this is where **separating vs. pooling equilibria** actually live |
| 15 | Type-space approach — types 557, conversion between state-space and type-space 563 | 557 | v1.2 — the type-space form is what most readers expect; §15 gives the conversion |

**Note on approach:** Bonanno uses the *state-space* approach (Part III machinery) rather than Harsanyi's type-space approach, and only converts to types in ch. 15. Most other literature and most LLM training data use type-space. The phase-2 schema should expose **types** as the caller-facing representation and cite ch. 15 for the equivalence, or callers will find the API alien.

---

## 4. Gap: repeated games are not in this book

`analyze_repeated_game` (v1.0 — grim-trigger discount-factor threshold, folk theorem) has **no chapter in Bonanno**. The table of contents covers static games, dynamic games, knowledge/belief, equilibrium refinements, and incomplete information; infinitely repeated games and the folk theorem are absent.

Consequences, all of which must be honored:

1. Do **not** cite Bonanno chapters in `analyze_repeated_game` output or in its MCP resource text.
2. Source the definitions and test cases elsewhere. **Decided 2026-07-27:** the source of record is Martin J. Osborne and Ariel Rubinstein, *A Course in Game Theory*, MIT Press, 1994, ch. 8. Payoff convention is the **discounted sum** with δ ∈ [0, 1), and the punishment is grim trigger with reversion to a pure-strategy stage Nash equilibrium. Mailath & Samuelson was the alternative; it goes further into imperfect monitoring than v1.0 needs.
3. Where the product copy says "anchored in Bonanno's textbook", the repeated-games tool is an explicit exception and should say so in its own description.

---

## 5. Test corpus

The book contains **165 exercises with complete published solutions**, distributed as:

| Ch. | Exercises | Exercises p. | Solutions p. | Usable as v1.0 tests |
|---|---|---|---|---|
| 1 | 23 | 43 | 51 | yes — dominance, IDSDS, pure Nash, second-price auction, pivotal mechanism |
| 2 | 13 | 84 | 92 | yes — backward induction, strategy enumeration |
| 3 | 14 | 132 | 139 | partial — only the perfect-information and subgame items |
| 4 | 14 | 177 | 181 | no — expected utility theory, not a solver |
| 5 | 15 | 205 | 213 | yes — mixed Nash, mixed dominance |
| 6 | 9 | 237 | 243 | partial |
| 7 | 14 | 272 | 279 | no |
| 8 | 12 | 316 | 323 | no |
| 9 | 7 | 344 | 348 | no |
| 10 | 8 | 375 | 380 | phase 2 |
| 11 | 6 | 400 | 406 | phase 2 |
| 12 | 10 | 448 | 456 | phase 2 |
| 13 | 8 | 489 | 497 | phase 2 |
| 14 | 7 | 535 | 541 | phase 2 |
| 15 | 4 | 566 | 570 | phase 2 |

**Plan:** transcribe the ch. 1, 2, and 5 exercises whose solutions are fully determined (roughly 50 items) into `game-theory-core/tests/fixtures/*.json`, each fixture carrying the book page of its published solution. These are the acceptance tests for v1.0 solvers — a solver is not done until it reproduces the textbook answers.

Transcribe only the game data and the answer, not the book's prose. CC BY-NC-ND permits neither redistribution of the text nor derivative works; encoding a payoff matrix as JSON and citing the page is fine, copying the exercise text into the repository is not.
