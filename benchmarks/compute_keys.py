"""One-off: compute ground-truth answers for every scenario via the server itself.

Not part of the benchmark run path — used when authoring/editing scenarios to
keep assertions in sync with what the server actually returns.
"""

import asyncio
import json

from harness.mcp_bridge import McpBridge

PD = {  # S1: hold price vs undercut
    "form": "matrix", "players": ["Us", "Rival"],
    "row_strategies": ["Hold", "Undercut"], "col_strategies": ["Hold", "Undercut"],
    "payoff_matrix": [[[8, 8], [0, 10]], [[10, 0], [2, 2]]], "payoff_kind": "cardinal",
}
ENTRY = {  # S2: entry deterrence tree
    "form": "extensive",
    "players": [{"id": 0, "name": "Entrant"}, {"id": 1, "name": "Incumbent"}],
    "root": 0,
    "nodes": [
        {"kind": "decision", "player": 0, "actions": [["In", 1], ["Out", 2]]},
        {"kind": "decision", "player": 1, "actions": [["Fight", 3], ["Accommodate", 4]]},
        {"kind": "terminal", "payoffs": [0, 2]},
        {"kind": "terminal", "payoffs": [-1, -1]},
        {"kind": "terminal", "payoffs": [1, 1]},
    ],
    "information_sets": [[0], [1]], "payoff_kind": "cardinal",
}
BOS = {  # S3: promo week coordination
    "form": "matrix", "players": ["Us", "Partner"],
    "row_strategies": ["WeekA", "WeekB"], "col_strategies": ["WeekA", "WeekB"],
    "payoff_matrix": [[[2, 1], [0, 0]], [[0, 0], [1, 2]]], "payoff_kind": "cardinal",
}
PENNIES = {  # S4: pick/counter-pick
    "form": "matrix", "players": ["Player", "Opponent"],
    "row_strategies": ["Aggro", "Safe"], "col_strategies": ["Counter", "Mirror"],
    "payoff_matrix": [[[1, -1], [-1, 1]], [[-1, 1], [1, -1]]], "payoff_kind": "cardinal",
}
STAG = {  # S5: platform coordination
    "form": "matrix", "players": ["Us", "Them"],
    "row_strategies": ["New", "Legacy"], "col_strategies": ["New", "Legacy"],
    "payoff_matrix": [[[4, 4], [0, 2]], [[2, 0], [2, 2]]], "payoff_kind": "cardinal",
}
WEAK = {  # S6: order-dependent weak dominance
    "form": "matrix", "players": ["Us", "Them"],
    "row_strategies": ["P", "Q", "R"], "col_strategies": ["X", "Y"],
    "payoff_matrix": [[[1, 1], [2, 0]], [[1, 1], [0, 2]], [[0, 0], [1, 1]]],
    "payoff_kind": "cardinal",
}
LAUNCH = {  # S7: two-branch sequential launch
    "form": "extensive",
    "players": [{"id": 0, "name": "Us"}, {"id": 1, "name": "Rival"}],
    "root": 0,
    "nodes": [
        {"kind": "decision", "player": 0, "actions": [["Aggressive", 1], ["Soft", 2]]},
        {"kind": "decision", "player": 1, "actions": [["Match", 3], ["Ignore", 4]]},
        {"kind": "decision", "player": 1, "actions": [["Match", 5], ["Ignore", 6]]},
        {"kind": "terminal", "payoffs": [3, 2]},
        {"kind": "terminal", "payoffs": [4, 1]},
        {"kind": "terminal", "payoffs": [1, 3]},
        {"kind": "terminal", "payoffs": [2, 4]},
    ],
    "information_sets": [[0], [1], [2]], "payoff_kind": "cardinal",
}
CHICKEN = {  # S8: budget standoff
    "form": "matrix", "players": ["Eng", "Design"],
    "row_strategies": ["Escalate", "Yield"], "col_strategies": ["Escalate", "Yield"],
    "payoff_matrix": [[[-2, -2], [2, -1]], [[-1, 2], [0, 0]]], "payoff_kind": "cardinal",
}

CALLS = [
    ("S1 repeated", "analyze_repeated_game", {"game": PD, "target": [0, 0]}),
    ("S1 structure", "analyze_payoff_structure", {"game": PD}),
    ("S2 BI", "solve_backward_induction", {"game": ENTRY}),
    ("S3 mixed", "solve_mixed_nash", {"game": BOS}),
    ("S4 mixed", "solve_mixed_nash", {"game": PENNIES}),
    ("S4 structure", "analyze_payoff_structure", {"game": PENNIES}),
    ("S5 pure", "solve_pure_nash", {"game": STAG}),
    ("S5 structure", "analyze_payoff_structure", {"game": STAG}),
    ("S6 weak", "solve_dominance", {"game": WEAK, "mode": "weak"}),
    ("S6 both", "solve_dominance", {"game": WEAK, "mode": "both"}),
    ("S7 convert", "convert_form", {"game": LAUNCH}),
    ("S8 mixed", "solve_mixed_nash", {"game": CHICKEN}),
    ("S8 pure", "solve_pure_nash", {"game": CHICKEN}),
    ("S8 structure", "analyze_payoff_structure", {"game": CHICKEN}),
]


async def main():
    async with McpBridge("../target/release/game-theory-mcp") as b:
        for label, tool, args in CALLS:
            r = await b.call(tool, args)
            print(f"\n===== {label} ({tool}) err={r.is_error}")
            try:
                pretty = json.dumps(json.loads(r.text), indent=1)
            except (json.JSONDecodeError, TypeError):
                pretty = r.text
            print(pretty[:1400])


if __name__ == "__main__":
    asyncio.run(main())
