"""Score raw transcripts against scenario assertions; emit summary.md + summary.json."""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path

import yaml

BENCH_ROOT = Path(__file__).resolve().parent.parent


def load_assertions() -> dict[str, dict]:
    out = {}
    for f in sorted((BENCH_ROOT / "scenarios").glob("*.yaml")):
        scn = yaml.safe_load(f.read_text())
        out[scn["id"]] = scn
    return out


def score_text(text: str, assertion: dict) -> bool:
    return any(re.search(p, text, re.IGNORECASE) for p in assertion["any_of"])


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tag", default="latest")
    ap.add_argument("--models", nargs="*", default=None,
                    help="restrict scoring to these model names")
    args = ap.parse_args()

    out_dir = BENCH_ROOT / "results" / args.tag
    raw_dir = out_dir / "raw"
    scenarios = load_assertions()

    # cell[(scenario, model, arm)] = list of per-trial dicts
    cells: dict[tuple[str, str, str], list[dict]] = defaultdict(list)
    for f in sorted(raw_dir.glob("*.json")):
        tr = json.loads(f.read_text())
        if args.models and tr["model"] not in args.models:
            continue
        scn = scenarios[tr["scenario_id"]]
        assertions = scn.get("assertions", [])
        if tr["error"] or not tr["final_text"]:
            matched = {a["id"]: False for a in assertions}
        else:
            matched = {
                a["id"]: score_text(tr["final_text"], a) for a in assertions
            }
        expected_tools = set(scn.get("expected_tools", []))
        used_tools = {tc["name"] for tc in tr["tool_calls"]}
        cells[(tr["scenario_id"], tr["model"], tr["arm"])].append(
            {
                "trial": tr["trial"],
                "score": sum(matched.values()) / max(len(matched), 1),
                "assertions": matched,
                "tool_calls": len(tr["tool_calls"]),
                "expected_tools_hit": sorted(expected_tools & used_tools),
                "expected_tools_missed": sorted(expected_tools - used_tools),
                "input_tokens": tr["usage"]["input_tokens"],
                "output_tokens": tr["usage"]["output_tokens"],
                "wall_seconds": tr["wall_seconds"],
                "error": tr["error"],
            }
        )

    # aggregate
    table: dict[str, dict] = {}
    for (sid, model, arm), trials in sorted(cells.items()):
        key = f"{sid}|{model}|{arm}"
        table[key] = {
            "trials": len(trials),
            "mean_score": round(sum(t["score"] for t in trials) / len(trials), 3),
            "mean_tool_calls": round(sum(t["tool_calls"] for t in trials) / len(trials), 2),
            "errors": sum(1 for t in trials if t["error"]),
            "mean_output_tokens": round(
                sum(t["output_tokens"] for t in trials) / len(trials)
            ),
            "per_trial": trials,
        }

    (out_dir / "summary.json").write_text(json.dumps(table, indent=2))

    lines = [
        "# Benchmark summary",
        "",
        "Score = fraction of exact-answer assertions matched in the final answer.",
        "",
        "| Scenario | Model | Arm | Score | Tool calls | Errors |",
        "|---|---|---|---|---|---|",
    ]
    for key, agg in table.items():
        sid, model, arm = key.split("|")
        short = model.replace("deepseek-", "")
        lines.append(
            f"| {sid} | {short} | {arm} | {agg['mean_score']:.2f} "
            f"({agg['trials']} trials) | {agg['mean_tool_calls']} | {agg['errors']} |"
        )
    (out_dir / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    print(f"\nwrote {out_dir}/summary.md")


if __name__ == "__main__":
    main()
