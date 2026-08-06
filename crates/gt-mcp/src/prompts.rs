//! Formalization prompts.
//!
//! The design dropped a natural-language parsing tool: this server never
//! parses prose, it computes on a formal game. That left a gap between "a
//! situation described in words" and "a `GameJson` some tool can accept", and
//! these prompts are what fills it. They run entirely on the host side -- each
//! one is guidance text with the caller's scenario interpolated, and no
//! arithmetic happens here.
//!
//! Each prompt walks the same four decisions, because those are the four that
//! go wrong: who the players are, what each one's strategies are, whether the
//! payoffs are ordinal or cardinal, and which tool to hand the result to. Each
//! carries a worked example, because the shape of a correct `GameJson` is
//! easier to copy than to describe.

use rmcp::model::JsonObject;
use rmcp::model::{GetPromptResult, Prompt, PromptArgument, PromptMessage, Role};
use rmcp::ErrorData;

/// One declared argument.
pub struct ArgSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

/// A prompt: metadata plus a renderer over its arguments.
pub struct PromptSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub arguments: &'static [ArgSpec],
    render: fn(&Args) -> String,
}

/// The arguments a `prompts/get` supplied, with a fallback for each missing
/// one. Missing optional arguments become an explicit instruction to the host
/// rather than an empty hole in the text.
pub struct Args<'a> {
    raw: Option<&'a JsonObject>,
}

impl Args<'_> {
    fn get(&self, name: &str, fallback: &'static str) -> String {
        self.raw
            .and_then(|m| m.get(name))
            .map(|v| match v {
                // A host may send a number or a bool where a string was
                // declared; render it rather than dropping the argument.
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string())
    }
}

impl PromptSpec {
    fn advertised(&self) -> Prompt {
        let args: Vec<PromptArgument> = self
            .arguments
            .iter()
            .map(|a| {
                PromptArgument::new(a.name)
                    .with_description(a.description)
                    .with_required(a.required)
            })
            .collect();
        Prompt::new(self.name, Some(self.description), Some(args)).with_title(self.title)
    }

    /// Render this prompt's text against the supplied arguments.
    pub fn render(&self, arguments: Option<&JsonObject>) -> String {
        (self.render)(&Args { raw: arguments })
    }
}

/// The three prompts named by the design.
pub static PROMPTS: &[PromptSpec] = &[
    PromptSpec {
        name: "formalize_scenario",
        title: "Formalize a scenario as a game",
        description: "Turn a situation described in words into a validated game this server \
                      can solve. Walks through choosing players, enumerating strategies, \
                      deciding ordinal versus cardinal payoffs, and picking the first tool to \
                      call.",
        arguments: &[
            ArgSpec {
                name: "scenario",
                description: "The situation to formalize, in plain language.",
                required: true,
            },
            ArgSpec {
                name: "known_payoffs",
                description: "Any payoff numbers already known, and what units they are in \
                              (dollars, utility, rank order). Leave out if unknown.",
                required: false,
            },
        ],
        render: formalize_scenario,
    },
    PromptSpec {
        name: "analyze_competitive_dynamic",
        title: "Analyze a competitive dynamic",
        description: "Diagnose a rivalry -- pricing, entry, an arms race, a standards fight -- \
                      by formalizing it and then identifying which structural question is \
                      actually being asked, so the right solver is called rather than all of \
                      them.",
        arguments: &[
            ArgSpec {
                name: "scenario",
                description: "The competitive situation, in plain language.",
                required: true,
            },
            ArgSpec {
                name: "question",
                description: "What the user actually wants to know (for example \"will prices \
                              collapse?\"). Leave out to analyze the structure generally.",
                required: false,
            },
        ],
        render: analyze_competitive_dynamic,
    },
    PromptSpec {
        name: "design_incentive_scheme",
        title: "Design an incentive scheme",
        description: "Work backwards from a desired outcome: formalize the game as it stands, \
                      confirm the desired profile is not currently an equilibrium, then find \
                      what change of payoffs or repetition would make it one.",
        arguments: &[
            ArgSpec {
                name: "scenario",
                description: "The situation whose incentives are to be changed.",
                required: true,
            },
            ArgSpec {
                name: "desired_outcome",
                description: "The behavior the scheme should make self-enforcing. Leave out to \
                              have it inferred from the scenario.",
                required: false,
            },
        ],
        render: design_incentive_scheme,
    },
];

/// Shared opening: the four decisions, spelled out once.
const FOUR_DECISIONS: &str = "\
## The four decisions

**1. Players.** A player is anyone whose choice affects someone else's payoff \
and who has a payoff of their own. Nature is not a player, and neither is \
anyone whose behavior is fixed -- fold those into the payoffs instead. Two \
players is the well-supported case: `solve_mixed_nash` requires exactly two, \
and `analyze_repeated_game` requires exactly two. `solve_pure_nash`, \
`solve_dominance`, and `analyze_payoff_structure` take any number.

**2. Strategies.** Each player needs a finite, explicitly listed set of \
strategies, and every combination of them must have a defined outcome. Coarsen \
until that is true: \"price high / price low\" is a usable strategy set, \
\"choose any price\" is not, and `validate_game` will reject an unbounded set \
rather than guess a discretization. In a sequential game a strategy is a \
complete contingent plan -- an action at every one of that player's decision \
nodes, including nodes that plan itself rules out -- not a path through the \
tree.

**3. Ordinal or cardinal.** Set `payoff_kind: \"ordinal\"` when you only know \
how each player ranks the outcomes. Set `\"cardinal\"` when the numbers are \
utilities whose averages mean something. This is a claim, not a format: it \
licenses every tool here that takes an expectation. Mixed strategies, repeated \
games, and mixed dominance all require cardinal payoffs and refuse ordinal \
input with `OrdinalPayoffsRejected` rather than returning a number that looks \
plausible. If the payoffs are dollars and no risk attitude has been stated, \
say so when reporting -- treating dollars as utilities assumes risk neutrality.

**4. Which tool.** Call `validate_game` first, always: it reports every \
problem at once, and a game that fails validation will fail every solver the \
same way. Then choose by the question, not by habit -- see below.
";

fn header(title: &str, scenario: &str) -> String {
    format!("# {title}\n\n## Scenario\n\n{scenario}\n\n")
}

fn formalize_scenario(a: &Args) -> String {
    let scenario = a.get(
        "scenario",
        "(none supplied -- ask the user to describe the situation before continuing)",
    );
    let payoffs = a.get(
        "known_payoffs",
        "(none supplied -- infer a ranking from the scenario and state the assumption \
         explicitly in your answer)",
    );
    format!(
        "{}## Known payoffs\n\n{payoffs}\n\n\
Formalize the scenario above as a game, then compute. Do not answer from \
intuition about what the situation resembles: the point of this server is that \
the arithmetic is exact, so get the game right and let the tools decide.\n\n\
{FOUR_DECISIONS}\n\
## Routing

- \"What will happen?\" → `solve_pure_nash`, then `solve_mixed_nash` if there \
  are no pure equilibria and payoffs are cardinal.
- \"Is there an obvious move?\" → `solve_dominance`.
- \"Someone moves first\" → build an `extensive` game and call \
  `solve_backward_induction`.
- \"Is this like a known situation?\" → `analyze_payoff_structure`.
- \"I already believe the answer is X\" → `verify_equilibrium`, which will \
  either confirm it or name the deviation that refutes it.

## Worked example

Scenario: two neighboring cafes each choose to run a discount or not. Both \
discounting splits the same customers at lower margin; one discounting alone \
takes the other's trade; neither discounting is comfortable for both.

Players: the two cafes. Strategies: `Discount`, `Hold` for each. The scenario \
gives a ranking (alone-discounting best, both-holding next, both-discounting \
next, being undercut worst) and no risk attitude, so this could be ordinal -- \
but pinning utilities lets the mixed and repeated tools run, so declare \
cardinal and say that the numbers are stylized:

```json
{{
  \"form\": \"matrix\",
  \"players\": [\"CafeA\", \"CafeB\"],
  \"row_strategies\": [\"Hold\", \"Discount\"],
  \"col_strategies\": [\"Hold\", \"Discount\"],
  \"payoff_matrix\": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
  \"payoff_kind\": \"cardinal\"
}}
```

Call `validate_game` on it, then `analyze_payoff_structure` (it classifies as \
`prisoners_dilemma`), then `solve_pure_nash` (unique equilibrium: both \
discount). Report the equilibrium *and* the fact that both cafes prefer the \
mutual-hold profile to it -- that gap is the finding, and it is what \
`gt://concepts/nash` warns about.

Read `gt://concepts/nash` and `gt://concepts/archetypes` if you need the \
concepts before choosing.",
        header("Formalize this scenario", &scenario)
    )
}

fn analyze_competitive_dynamic(a: &Args) -> String {
    let scenario = a.get(
        "scenario",
        "(none supplied -- ask the user to describe the rivalry before continuing)",
    );
    let question = a.get(
        "question",
        "(none supplied -- analyze the structure and report what it implies)",
    );
    format!(
        "{}## Question to answer\n\n{question}\n\n\
Diagnose this rivalry by formalizing it first. The common failure here is \
naming a famous game from the story rather than from the payoffs: whether \
something is a prisoner's dilemma is a fact about the preference ordering, and \
`analyze_payoff_structure` will tell you, with the criteria it checked.\n\n\
{FOUR_DECISIONS}\n\
## Routing for competitive questions

- \"Will they undercut each other?\" → `analyze_payoff_structure` for the \
  archetype, then `solve_pure_nash`.
- \"Can the cooperative outcome hold up over time?\" → `analyze_repeated_game` \
  with the cooperative profile as `target`. It returns the critical discount \
  factor delta*. Requires exactly two players and cardinal payoffs.
- \"Who moves first, and does it matter?\" → model it as an `extensive` game \
  and call `solve_backward_induction`; then reorder the movers and run it again. \
  A first-mover advantage is a difference between those two answers, not an \
  assumption.
- \"There is no stable outcome\" → if `solve_pure_nash` returns an empty list, \
  that is the finding. Follow with `solve_mixed_nash` to get the equilibrium in \
  mixtures, which is where the churn lives.

## Worked example

Scenario: two airlines on one route each choose `Aggressive` or `Passive` \
capacity. Matching aggression burns both; matching passivity is profitable for \
both; a lone aggressor wins the route but a mutual fight is the worst outcome \
for each.

Note this is *not* a prisoner's dilemma even though it reads like a price war: \
mutual aggression is worst for both, so aggression is not dominant. It is \
chicken, with two asymmetric pure equilibria.

```json
{{
  \"form\": \"matrix\",
  \"players\": [\"Alpha\", \"Beta\"],
  \"row_strategies\": [\"Passive\", \"Aggressive\"],
  \"col_strategies\": [\"Passive\", \"Aggressive\"],
  \"payoff_matrix\": [[[3.0, 3.0], [1.0, 4.0]], [[4.0, 1.0], [0.0, 0.0]]],
  \"payoff_kind\": \"cardinal\"
}}
```

`analyze_payoff_structure` returns `chicken`. `solve_pure_nash` returns two \
equilibria, (Aggressive, Passive) and (Passive, Aggressive) -- so the model \
does not predict which airline backs down, and saying so is the honest answer. \
`solve_mixed_nash` gives the third, symmetric equilibrium, whose probability of \
a mutual fight is the number worth quoting.

Read `gt://concepts/archetypes` and `gt://concepts/repeated-games` for the \
concepts behind those calls.",
        header("Analyze this competitive dynamic", &scenario)
    )
}

fn design_incentive_scheme(a: &Args) -> String {
    let scenario = a.get(
        "scenario",
        "(none supplied -- ask the user to describe the situation before continuing)",
    );
    let desired = a.get(
        "desired_outcome",
        "(none supplied -- infer the intended behavior from the scenario and state it \
         explicitly before proceeding)",
    );
    format!(
        "{}## Desired outcome\n\n{desired}\n\n\
Design incentives by working backwards, and verify at each step rather than \
reasoning it through in prose.\n\n\
1. Formalize the game **as it stands today**, using the four decisions below.\n\
2. Call `verify_equilibrium` on the desired profile. If it already holds, \
   there is nothing to design and the problem is elsewhere -- say so.\n\
3. Read the refutation. It names the player who deviates and what the \
   deviation pays. That number is the size of the incentive problem, and any \
   scheme has to close exactly that gap.\n\
4. Change the game and re-verify. Two levers are available here: alter the \
   payoffs (a bonus, a penalty, a bond) and re-run `verify_equilibrium`, or \
   invoke repetition and run `analyze_repeated_game` for the discount factor \
   that makes the profile self-enforcing without changing any payoff.\n\n\
{FOUR_DECISIONS}\n\
## Worked example

Scenario: two firms in a joint venture each choose `Invest` or `Shirk`. Mutual \
investment pays both well; shirking while the partner invests pays best of all \
for the shirker; mutual shirking pays little.

```json
{{
  \"form\": \"matrix\",
  \"players\": [\"FirmA\", \"FirmB\"],
  \"row_strategies\": [\"Invest\", \"Shirk\"],
  \"col_strategies\": [\"Invest\", \"Shirk\"],
  \"payoff_matrix\": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
  \"payoff_kind\": \"cardinal\"
}}
```

Step 2: `verify_equilibrium` with `concept: \"pure_nash\"` and \
`profile: [0, 0]` returns `holds: false`, with a deviation to `Shirk` worth 4 \
against 3. The gap is 1.

Step 4a, change the payoffs: a contractual penalty of more than 1 on unilateral \
shirking makes the deviation unprofitable. Edit the matrix to \
`[[[3.0, 3.0], [0.0, 2.5]], [[2.5, 0.0], [1.0, 1.0]]]` and re-run \
`verify_equilibrium`; it now holds. Note that the penalty must be *credible* -- \
if enforcing it is itself costly, model the enforcer as a third player or as a \
sequential move and check with `solve_backward_induction`.

Step 4b, change the horizon instead: `analyze_repeated_game` on the original \
matrix with `target: [0, 0]` returns the critical discount factor delta*. Above \
it, grim trigger sustains mutual investment with no contract at all. Quote \
delta* as the exact fraction the tool returns and translate it into how much \
the partners must value the next period.

Report both levers, with the numbers. \"Add a penalty\" without the threshold \
it must clear is not a design.

Read `gt://concepts/repeated-games` for the payoff convention behind delta*, \
and note that it is the one concept here not anchored in Bonanno.",
        header("Design an incentive scheme", &scenario)
    )
}

/// Look a prompt up by name.
pub fn find(name: &str) -> Option<&'static PromptSpec> {
    PROMPTS.iter().find(|p| p.name == name)
}

/// Everything `prompts/list` advertises.
pub fn list() -> Vec<Prompt> {
    PROMPTS.iter().map(PromptSpec::advertised).collect()
}

/// Render one prompt, or fail loudly.
///
/// An unknown name is a JSON-RPC error. A host that misspells a prompt should
/// find out, not receive an empty conversation.
pub fn get(name: &str, arguments: Option<&JsonObject>) -> Result<GetPromptResult, ErrorData> {
    let spec = find(name).ok_or_else(|| {
        ErrorData::invalid_params(
            format!("no such prompt: {name}"),
            Some(serde_json::json!({
                "name": name,
                "available": PROMPTS.iter().map(|p| p.name).collect::<Vec<_>>(),
            })),
        )
    })?;
    let missing: Vec<&str> = spec
        .arguments
        .iter()
        .filter(|a| a.required)
        .map(|a| a.name)
        .filter(|n| {
            !arguments.is_some_and(|m| {
                m.get(*n)
                    .is_some_and(|v| v.as_str().is_none_or(|s| !s.trim().is_empty()))
            })
        })
        .collect();
    if !missing.is_empty() {
        return Err(ErrorData::invalid_params(
            format!(
                "prompt {name} is missing required argument(s): {}",
                missing.join(", ")
            ),
            Some(serde_json::json!({ "name": name, "missing": missing })),
        ));
    }
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        Role::User,
        spec.render(arguments),
    )])
    .with_description(spec.description))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> JsonObject {
        pairs
            .iter()
            .map(|(k, v)| {
                (
                    (*k).to_string(),
                    serde_json::Value::String((*v).to_string()),
                )
            })
            .collect()
    }

    fn scenario() -> JsonObject {
        args(&[(
            "scenario",
            "Two bakeries decide whether to open on Sundays.",
        )])
    }

    #[test]
    fn the_three_designed_prompts_are_the_ones_served() {
        let mut names: Vec<&str> = PROMPTS.iter().map(|p| p.name).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "analyze_competitive_dynamic",
                "design_incentive_scheme",
                "formalize_scenario",
            ]
        );
    }

    #[test]
    fn every_listed_prompt_is_retrievable() {
        for advertised in list() {
            let result = get(&advertised.name, Some(&scenario()))
                .unwrap_or_else(|e| panic!("{} listed but not gettable: {e:?}", advertised.name));
            assert_eq!(result.messages.len(), 1);
            assert!(result.description.is_some_and(|d| !d.is_empty()));
        }
    }

    #[test]
    fn every_listed_prompt_declares_its_arguments() {
        for p in list() {
            assert!(p.title.as_ref().is_some_and(|t| !t.is_empty()), "{p:?}");
            assert!(
                p.description.as_ref().is_some_and(|d| !d.is_empty()),
                "{p:?}"
            );
            let declared = p.arguments.as_ref().expect("arguments must be declared");
            assert!(
                declared.iter().any(|a| a.required == Some(true)),
                "{} declares no required argument",
                p.name
            );
        }
    }

    /// Guidance that does not cover all four decisions leaves the host to
    /// guess exactly where hosts guess wrong.
    #[test]
    fn every_prompt_covers_the_four_decisions_and_routes_to_a_tool() {
        for p in PROMPTS {
            let text = p.render(Some(&scenario()));
            for needle in [
                "Players",
                "Strategies",
                "ordinal",
                "cardinal",
                "payoff_kind",
            ] {
                assert!(text.contains(needle), "{} omits {needle}", p.name);
            }
            assert!(
                text.contains("validate_game"),
                "{} omits validation",
                p.name
            );
            assert!(
                text.contains("solve_") || text.contains("analyze_") || text.contains("verify_"),
                "{} names no tool to call",
                p.name
            );
        }
    }

    #[test]
    fn every_prompt_carries_a_worked_example() {
        for p in PROMPTS {
            let text = p.render(Some(&scenario()));
            assert!(text.contains("Worked example"), "{}", p.name);
            // A worked example without a game body is not one a host can copy.
            assert!(text.contains("\"payoff_matrix\""), "{}", p.name);
            assert!(text.contains("\"payoff_kind\""), "{}", p.name);
        }
    }

    #[test]
    fn the_scenario_argument_is_interpolated() {
        let text = get("formalize_scenario", Some(&scenario()))
            .unwrap()
            .messages[0]
            .content
            .as_text()
            .unwrap()
            .text
            .clone();
        assert!(text.contains("Two bakeries decide whether to open on Sundays."));
    }

    #[test]
    fn an_absent_optional_argument_becomes_an_instruction_not_a_hole() {
        let text = PROMPTS[0].render(Some(&scenario()));
        assert!(text.contains("none supplied"), "{text}");
        assert!(!text.contains("\n\n\n\n"), "left an empty section: {text}");
    }

    #[test]
    fn a_supplied_optional_argument_wins_over_the_fallback() {
        let text = PROMPTS[0].render(Some(&args(&[
            ("scenario", "Two bakeries."),
            ("known_payoffs", "Profit in thousands of euros."),
        ])));
        assert!(text.contains("Profit in thousands of euros."));
        assert!(
            !text.contains("(none supplied -- infer a ranking"),
            "{text}"
        );
    }

    #[test]
    fn an_unknown_prompt_name_is_an_error() {
        let err = get("solve_everything", Some(&scenario())).unwrap_err();
        assert!(err.message.contains("solve_everything"), "{err:?}");
        let data = err.data.expect("error should name the available prompts");
        assert_eq!(data["available"].as_array().unwrap().len(), PROMPTS.len());
    }

    #[test]
    fn a_missing_required_argument_is_an_error() {
        let err = get("formalize_scenario", None).unwrap_err();
        assert!(err.message.contains("scenario"), "{err:?}");
        let err = get("formalize_scenario", Some(&args(&[("scenario", "  ")]))).unwrap_err();
        assert!(err.message.contains("scenario"), "{err:?}");
    }
}
