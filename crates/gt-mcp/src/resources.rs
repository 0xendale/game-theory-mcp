//! Concept resources at `gt://concepts/{slug}`.
//!
//! Six short explanations of the concepts this server computes, each naming
//! the tool that computes it and each carrying a textbook citation. The point
//! is to make the product's "textbook-anchored" claim inspectable: a host can
//! read the anchor rather than take it on faith.
//!
//! Every word of the prose below is written for this repository. The cited
//! books are referenced as published works by chapter and page; none of their
//! text is reproduced. `repeated-games` is the one concept with no chapter in
//! Bonanno, and it says so and cites Osborne and Rubinstein instead.

use rmcp::model::{ReadResourceResult, Resource, ResourceContents};
use rmcp::ErrorData;

/// One concept resource. `citation` is kept out of `body` so a test can assert
/// that every concept resolves to a real anchor, independently of the prose.
pub struct Concept {
    /// The path segment after `gt://concepts/`.
    pub slug: &'static str,
    /// Human-readable display title.
    pub title: &'static str,
    /// One line, shown in `resources/list`.
    pub summary: &'static str,
    /// The explanation itself.
    pub body: &'static str,
    /// The published source, by chapter and page.
    pub citation: &'static str,
}

impl Concept {
    /// The resource URI this concept is served at.
    pub fn uri(&self) -> String {
        format!("gt://concepts/{}", self.slug)
    }

    /// Body plus the resolved citation line -- what a host actually reads.
    pub fn text(&self) -> String {
        format!("{}\n\nSource: {}\n", self.body.trim_end(), self.citation())
    }

    /// The full citation line.
    ///
    /// The `citation` field holds either a bare Bonanno anchor or a complete
    /// standalone citation; this joins the former to the shared book
    /// reference so the anchor text stays readable in the table above.
    pub fn citation(&self) -> String {
        if self.citation == OSBORNE_RUBINSTEIN {
            self.citation.to_string()
        } else {
            format!(
                "{BONANNO}, {} — <http://www.econ.ucdavis.edu/faculty/bonanno/>",
                self.citation
            )
        }
    }

    fn advertised(&self) -> Resource {
        Resource::new(self.uri(), self.slug)
            .with_title(self.title)
            .with_description(self.summary)
            .with_mime_type("text/markdown")
    }
}

const BONANNO: &str = "Giacomo Bonanno, *Game Theory: An open access textbook with 165 solved \
                       exercises*, UC Davis, 2015";

/// The catalog, in the order the design lists it.
pub static CONCEPTS: &[Concept] = &[
    Concept {
        slug: "nash",
        title: "Nash equilibrium",
        summary: "Mutual best responses: no player gains by deviating alone.",
        body: "# Nash equilibrium

A strategy profile is a Nash equilibrium when no single player can raise their \
own payoff by switching to a different strategy while everyone else keeps \
theirs fixed. That is the whole definition: a fixed point against unilateral \
deviation.

What it is not:

- Not the best outcome for the group. A game can have exactly one equilibrium \
  and have it be worse for every player than some other profile.
- Not a prediction that players will agree on one when several exist. The \
  concept says nothing about which equilibrium gets played.
- Not guaranteed to exist in pure strategies. A game may have none, one, or \
  many; in mixed strategies a finite game always has at least one.
- Not the same as a dominant-strategy profile, which is a strictly stronger \
  requirement -- see `gt://concepts/dominance`.

Tools that compute it:

- `solve_pure_nash` enumerates every pure-strategy equilibrium of a \
  strategic-form game. A game with none returns an empty list, which is an \
  answer and not an error.
- `solve_mixed_nash` covers the mixed case for two players -- see \
  `gt://concepts/mixed-strategies`.
- `verify_equilibrium` with `concept: \"pure_nash\"` checks one profile you \
  already have in hand, and when it fails it names the player and the \
  deviation that refutes it.

Payoff kind: pure-strategy Nash needs only ordinal payoffs. The definition \
compares each player's own payoffs against one another and never averages \
them, so ranks suffice.",
        citation: BONANNO_NASH,
    },
    Concept {
        slug: "dominance",
        title: "Dominance and iterated deletion",
        summary: "Strategies a player should never use, and what deleting them leaves.",
        body: "# Dominance

One strategy strictly dominates another, for the same player, when it pays \
strictly more against every combination of what the others might do. Weak \
dominance relaxes that to \"at least as much everywhere, and strictly more \
somewhere\". Both are statements about one player's own payoffs only; no \
comparison across players is involved.

Iterated deletion removes dominated strategies, then re-examines the smaller \
game, and repeats. Deleting *strictly* dominated strategies is \
order-independent -- whatever order you delete in, the same reduced game is \
left. Deleting *weakly* dominated strategies is not: different orders can \
leave different games, and can discard equilibria. This server reports which \
mode was used and warns on the weak path rather than quietly picking an order \
for you.

What it is not:

- Not the same as being a bad strategy in equilibrium. A strategy can survive \
  every round of deletion and still never be played.
- Not confined to pure comparison. On cardinal payoffs a pure strategy can be \
  strictly dominated by a *mixture* of that player's other strategies while no \
  single pure strategy dominates it. Checking only pairs of pure strategies \
  understates dominance.
- Not a substitute for equilibrium. When deletion leaves more than one profile \
  standing, it has not solved the game.

Tools that compute it:

- `solve_dominance` runs the deletion, with the mode selected explicitly. On \
  cardinal games it also tests dominance by mixed strategies; on ordinal games \
  only pure dominance is defined, so only that is checked.
- `verify_equilibrium` with `concept: \"dominant_strategy\"` checks a claim \
  that a specific profile is dominant.

Payoff kind: pure dominance is ordinal. Mixed dominance requires cardinal \
payoffs, because deciding whether a mixture beats a pure strategy means \
averaging utilities.",
        citation: BONANNO_DOMINANCE,
    },
    Concept {
        slug: "subgame-perfect",
        title: "Backward induction and subgame perfection",
        summary:
            "Equilibrium that stays an equilibrium in every subgame, ruling out empty threats.",
        body: "# Subgame-perfect equilibrium

In a game tree, a Nash equilibrium can rest on a threat that the threatening \
player would never actually want to carry out, because the node where it would \
be carried out is never reached. Subgame perfection rules those out: the \
profile must remain an equilibrium of every subgame, including the ones that \
equilibrium play never visits.

For a finite tree with perfect information, backward induction computes it. \
Start at the last decision nodes, fix each player's best action there, fold the \
resulting payoff up into the parent, and continue to the root.

What it is not:

- Not a path through the tree. A strategy is a complete contingent plan: it \
  names an action at *every* decision node belonging to that player, including \
  nodes ruled out by the player's own earlier choices. Supplying only the \
  actions along the equilibrium path is the most common way to get a wrong \
  answer here, and it is why `convert_form` produces more strategies than a \
  reader usually expects.
- Not a weaker condition than Nash. Every subgame-perfect equilibrium is a \
  Nash equilibrium of the corresponding strategic form; the converse fails.
- Not available under imperfect information in this version. Non-singleton \
  information sets are accepted by the schema and refused by the solvers with \
  `ImperfectInformationUnsupported` rather than being silently treated as \
  singletons.

Tools that compute it:

- `solve_backward_induction` folds a perfect-information tree and returns the \
  equilibrium plan for each player together with the resulting payoffs.
- `convert_form` turns a tree into its strategic form, enumerating complete \
  contingent plans, so the same game can be handed to the strategic-form \
  solvers.
- `verify_equilibrium` with `concept: \"spe\"` checks one claimed plan profile.

Payoff kind: backward induction on a perfect-information tree with no chance \
moves only compares payoffs, so ordinal payoffs are enough.",
        citation: BONANNO_SPE,
    },
    Concept {
        slug: "mixed-strategies",
        title: "Mixed strategies",
        summary: "Randomizing over own strategies, and why it demands cardinal payoffs.",
        body: "# Mixed strategies

A mixed strategy is a probability distribution over one player's own pure \
strategies. A mixed-strategy profile is one such distribution per player, and \
it is an equilibrium when no player can do better by shifting probability \
around -- equivalently, when every pure strategy a player puts positive \
probability on yields that player the same expected payoff, and no strategy \
outside the support yields more. Those indifference conditions are the \
computation.

What it is not:

- Not a claim that anyone flips coins. It is the profile at which each player \
  is content, given a belief about the others that the equilibrium makes \
  correct.
- Not defined on ordinal payoffs. Ranks cannot be averaged: the expected value \
  of a rank is not a rank, and rescaling ranks would change the answer. Any \
  tool here that takes an expectation refuses ordinal input with \
  `OrdinalPayoffsRejected` rather than returning a number that looks fine.
- Not approximate. Equilibrium probabilities are rationals and cross the wire \
  as exact fraction strings such as `\"1/3\"`. Sending `\"0.5\"` where a \
  probability is expected is a malformed request, not a rounding convenience.

Tools that compute it:

- `solve_mixed_nash` enumerates supports for two-player cardinal games and \
  solves the indifference conditions exactly. More than two players returns \
  `NPlayerMixedUnsupported` and points you at `solve_pure_nash`.
- `verify_equilibrium` with `concept: \"mixed_nash\"` checks a profile you \
  supply as one fraction vector per player, and reports the expected payoffs it \
  computed.
- `solve_dominance` on a cardinal game uses mixtures too, when testing whether \
  a pure strategy is strictly dominated.

Payoff kind: cardinal, always. Declaring `payoff_kind: \"cardinal\"` is a \
substantive claim that the numbers you supplied are utilities whose \
expectations mean something -- not merely a statement about how they are \
formatted.",
        citation: BONANNO_MIXED,
    },
    Concept {
        slug: "archetypes",
        title: "Payoff-structure archetypes",
        summary: "Named 2x2 patterns -- prisoner's dilemma, stag hunt, chicken, and the rest.",
        body: "# Archetypes

Some payoff structures recur often enough to have names. This server \
recognizes five: `prisoners_dilemma`, `stag_hunt`, `chicken`, \
`battle_of_the_sexes`, and `matching_pennies`. Anything that fits none of them \
comes back as `none_matched`, which is a result rather than a failure.

Classification is structural, not nominal. A prisoner's dilemma is any game \
whose iterated strict dominance leaves a single profile that some other profile \
Pareto-dominates; the story about prisoners is irrelevant to whether the label \
applies. Because of that, the classifier works on the *game* -- players, \
strategies, and preferences over outcomes together -- and not on the frame \
alone. The same players with the same strategies and the same list of outcomes \
constitute a different game once preferences over those outcomes change, and \
can land in a different archetype.

What it is not:

- Not a solution. A label tells you the shape of the conflict, not what anyone \
  should play. Use it to choose which solver to reach for.
- Not exhaustive. `none_matched` is common and correct for most games.
- Not a guess. The classifier reports which criteria were met and which \
  failed, so a near-miss is visible instead of being rounded to the nearest \
  famous game.

Tools that compute it:

- `analyze_payoff_structure` returns the archetype together with the criteria \
  behind the verdict, plus structural properties such as symmetry, zero-sum, \
  and Pareto-efficient profiles.
- `validate_game` first, if the game came from a caller rather than from \
  another tool -- classification presumes a well-formed game.

Payoff kind: the classification is ordinal. It depends on how each player ranks \
outcomes, not on the size of the gaps between them.",
        citation: BONANNO_ARCHETYPES,
    },
    Concept {
        slug: "repeated-games",
        title: "Infinitely repeated games",
        summary:
            "When patience sustains cooperation -- the one concept here not anchored in Bonanno.",
        body: "# Infinitely repeated games

Play a stage game over and over, with no last round, and profiles that are not \
equilibria of the stage game can become sustainable. Cooperation pays off \
forever; defecting pays off once and then triggers punishment. Whether the \
trade favors cooperation depends on how heavily future rounds are weighted.

This server uses the discounted sum with a discount factor delta in [0, 1), and \
grim trigger as the punishment: after any deviation, players revert forever to \
a pure-strategy Nash equilibrium of the stage game. Reverting to a stage \
equilibrium is what makes the threat credible -- carrying out the punishment is \
itself equilibrium behavior, so the threat is not one the punisher would want \
to abandon. The output is a critical discount factor delta*, above which the \
target profile is sustainable.

Note on the anchor: this is the one concept in this server that is **not** \
drawn from Bonanno's textbook, which does not cover infinitely repeated games \
or the folk theorem. Its source of record is different, and is cited below. \
Where this server's other tools point at a Bonanno chapter, \
`analyze_repeated_game` deliberately does not.

What it is not:

- Not a finitely repeated game. With a known last round, backward induction \
  from the final stage unravels the cooperative construction entirely. The \
  infinite horizon is load-bearing.
- Not a claim about actual play. delta* is a threshold on preferences, not a \
  forecast that players will cooperate.
- Not an average-payoff calculation. delta* depends on the payoff convention, \
  and the convention here is the discounted sum. A threshold computed under a \
  different convention is not comparable to this one.

Tools that compute it:

- `analyze_repeated_game` takes a two-player cardinal stage game and a target \
  profile, and returns delta* as an exact fraction along with each player's \
  target payoff, best one-shot deviation and its value, punishment payoff, \
  minmax value, and own threshold. Supply `discount_factor` as a fraction \
  string to be told directly whether the target holds at it.

Payoff kind: cardinal, always. A discounted sum of ranks means nothing, so \
ordinal input is refused with `OrdinalPayoffsRejected`.",
        citation: OSBORNE_RUBINSTEIN,
    },
];

// Anchors are named constants so every one can be checked against
// `docs/reference/bonanno-concept-map.md` in a single place. Only anchors that
// file actually records appear here -- a citation that does not resolve to a
// real chapter is worse than no citation at all.
const BONANNO_NASH: &str = "ch. 1 §1.6, p. 32";
const BONANNO_DOMINANCE: &str =
    "ch. 1 §1.2, p. 14 and §1.5, p. 28; mixed dominance ch. 5 §5.4, p. 201";
const BONANNO_SPE: &str =
    "ch. 2 §2.2, p. 69 and §2.3, p. 73; subgame-perfect equilibrium ch. 3 §3.4, p. 119";
const BONANNO_MIXED: &str = "ch. 5 §5.2, p. 190 and §5.3, p. 195; expected utility ch. 4 §4.1, \
                             p. 158";
const BONANNO_ARCHETYPES: &str = "ch. 1 §1.1, p. 6 and §1.6, p. 32";

const OSBORNE_RUBINSTEIN: &str = "Martin J. Osborne and Ariel Rubinstein, *A Course in Game \
                                  Theory*, MIT Press, 1994, ch. 8. Not covered by Bonanno's \
                                  textbook; this concept's anchor is deliberately a different \
                                  book.";

/// Look a concept up by its full URI.
pub fn find(uri: &str) -> Option<&'static Concept> {
    CONCEPTS.iter().find(|c| c.uri() == uri)
}

/// Everything `resources/list` advertises.
pub fn list() -> Vec<Resource> {
    CONCEPTS.iter().map(Concept::advertised).collect()
}

/// Serve one concept, or fail loudly.
///
/// An unknown URI is a JSON-RPC error. Returning an empty body would let a
/// typo look like a concept with nothing to say about it.
pub fn read(uri: &str) -> Result<ReadResourceResult, ErrorData> {
    let concept = find(uri).ok_or_else(|| {
        ErrorData::resource_not_found(
            format!("no such resource: {uri}"),
            Some(serde_json::json!({
                "uri": uri,
                "available": CONCEPTS.iter().map(Concept::uri).collect::<Vec<_>>(),
            })),
        )
    })?;
    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        concept.text(),
        concept.uri(),
    )
    .with_mime_type("text/markdown")]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_six_designed_concepts_are_the_ones_served() {
        let mut slugs: Vec<&str> = CONCEPTS.iter().map(|c| c.slug).collect();
        slugs.sort_unstable();
        assert_eq!(
            slugs,
            vec![
                "archetypes",
                "dominance",
                "mixed-strategies",
                "nash",
                "repeated-games",
                "subgame-perfect",
            ]
        );
    }

    #[test]
    fn every_listed_uri_is_readable() {
        for advertised in list() {
            let result = read(&advertised.uri)
                .unwrap_or_else(|e| panic!("{} listed but not readable: {e:?}", advertised.uri));
            assert_eq!(result.contents.len(), 1);
        }
    }

    #[test]
    fn every_listed_resource_is_titled_and_described() {
        for r in list() {
            assert!(r.title.as_ref().is_some_and(|t| !t.is_empty()), "{r:?}");
            assert!(
                r.description.as_ref().is_some_and(|d| !d.is_empty()),
                "{r:?}"
            );
            assert!(r.uri.starts_with("gt://concepts/"), "{r:?}");
        }
    }

    /// A body with no citation would make the "textbook-anchored" claim
    /// unfalsifiable, which is the one thing these resources exist to prevent.
    #[test]
    fn every_body_is_substantial_and_carries_its_citation() {
        for c in CONCEPTS {
            let text = c.text();
            assert!(
                text.len() > 400,
                "{} is too short to explain anything: {} chars",
                c.slug,
                text.len()
            );
            assert!(
                text.contains(&c.citation()),
                "{} does not carry its citation",
                c.slug
            );
        }
    }

    /// Only anchors recorded in `docs/reference/bonanno-concept-map.md` may be
    /// cited. A citation that does not resolve is worse than none.
    #[test]
    fn five_concepts_cite_bonanno_by_chapter_and_page() {
        for c in CONCEPTS.iter().filter(|c| c.slug != "repeated-games") {
            let line = c.citation();
            assert!(line.contains("Bonanno"), "{}: {line}", c.slug);
            assert!(line.contains("ch. "), "{} cites no chapter: {line}", c.slug);
            assert!(line.contains("p. "), "{} cites no page: {line}", c.slug);
        }
    }

    /// Bonanno has no repeated-games chapter, so citing one here would be a
    /// fabricated anchor. See `bonanno-concept-map.md` §4.
    #[test]
    fn repeated_games_cites_osborne_and_rubinstein_instead() {
        let c = find("gt://concepts/repeated-games").unwrap();
        let line = c.citation();
        assert!(line.contains("Osborne"), "{line}");
        assert!(line.contains("Rubinstein"), "{line}");
        assert!(line.contains("ch. 8"), "{line}");
        // It may name Bonanno only to disclaim him -- never with a page.
        assert!(
            !line.contains("p. "),
            "no Bonanno page may be attached here: {line}"
        );
        assert!(
            c.text().contains("not** drawn from Bonanno"),
            "{}",
            c.text()
        );
    }

    #[test]
    fn every_concept_names_a_tool_the_host_can_call() {
        for c in CONCEPTS {
            let body = c.body;
            assert!(
                body.contains("Tools that compute it:"),
                "{} routes the host nowhere",
                c.slug
            );
            assert!(
                body.contains("solve_") || body.contains("analyze_") || body.contains("verify_"),
                "{} names no tool",
                c.slug
            );
        }
    }

    #[test]
    fn an_unknown_uri_is_an_error_not_an_empty_body() {
        let err = read("gt://concepts/nonexistent").unwrap_err();
        assert!(err.message.contains("nonexistent"), "{err:?}");
        // The error lists what does exist, so a typo is self-correcting.
        let data = err.data.expect("error should name the available concepts");
        assert_eq!(data["available"].as_array().unwrap().len(), CONCEPTS.len());
    }

    #[test]
    fn a_bare_slug_is_not_mistaken_for_a_uri() {
        assert!(read("nash").is_err());
        assert!(read("gt://concepts/nash").is_ok());
    }
}
