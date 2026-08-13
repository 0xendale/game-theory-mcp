# Docs index

| File | What it is |
|---|---|
| [`reference/product-spec-source.md`](reference/product-spec-source.md) | The original product spec transcribed to markdown (§1–6), plus a review section (§7) recording every point where the delivered design departs from it and why. |
| [`reference/bonanno-concept-map.md`](reference/bonanno-concept-map.md) | Bonanno *Game Theory* textbook mapped chapter-by-chapter to tools and phases, with page anchors, the exercise-based test corpus plan, and an honest note on what the book does **not** cover. |
| [`demos/demo-01-price-war.md`](demos/demo-01-price-war.md) | Pricing-war scenario through the eyes of a growth marketing lead — repeated-game sustainability with an exact 1/4 threshold. |
| [`demos/demo-02-market-entry.md`](demos/demo-02-market-entry.md) | Market-entry scenario — backward induction showing an incumbent's fight threat is not credible. |
| [`demos/demo-03-promo-calendar.md`](demos/demo-03-promo-calendar.md) | Promo-calendar coordination under an embargo — the exact 2/3 : 1/3 mixed equilibrium. |

The demos are persona-driven benchmark transcripts: the question is asked by a
non-specialist in business language, and the MCP server's exact answers carry
the analysis. They regenerate with the harness in `benchmarks/`.

Reading order for someone new: `product-spec-source.md` §1–6 for the product intent, then §7 for the design decisions that changed it, then `bonanno-concept-map.md` §1–2 for the mathematics being implemented.

`product-spec-source.md` §7 is the authoritative record of design departures — dropping the natural-language parsing tool, the ordinal/cardinal payoff split, exact rational arithmetic, and the tool decomposition. Add an entry there when a new decision changes the shape of the product.
