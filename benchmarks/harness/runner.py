"""Run the benchmark matrix: scenarios x models x arms x trials."""

from __future__ import annotations

import argparse
import asyncio
import json
from pathlib import Path

import yaml
from openai import AsyncOpenAI

from .agent import run_trial

BENCH_ROOT = Path(__file__).resolve().parent.parent
REPO_ROOT = BENCH_ROOT.parent
DEFAULT_BINARY = REPO_ROOT / "target" / "release" / "game-theory-mcp"
MODELS = ["deepseek-v4-flash"]
ARMS = ["bare", "mcp"]
AUTH_JSON = Path.home() / ".local" / "share" / "opencode" / "auth.json"

PROVIDERS = {
    "opencode-go": {
        "base_url": "https://opencode.ai/zen/go/v1",
        "key": lambda data: data.get("opencode-go", {}).get("key"),
        "models": ["deepseek-v4-flash"],
    },
    "gemini": {
        "base_url": "https://generativelanguage.googleapis.com/v1beta/openai/",
        "key": lambda data: data.get("google", {}).get("key"),
        "models": ["gemini-3.6-flash"],
    },
    "copilot": {
        "base_url": "https://api.githubcopilot.com",
        "key": lambda data: (data.get("github-copilot") or {}).get("access"),
        "models": ["gpt-5-mini", "gpt-5.4-mini"],
        "headers": {
            "Copilot-Integration-Id": "opencode",
            "OpenAI-Intent": "copilot",
            "Editor-Version": "1.0.0",
        },
    },
}


def load_client(provider: str) -> AsyncOpenAI:
    import os

    cfg = PROVIDERS[provider]
    env_name = provider.upper().replace("-", "_") + "_API_KEY"
    key = os.environ.get(env_name)
    if not key and AUTH_JSON.exists():
        key = cfg["key"](json.loads(AUTH_JSON.read_text()))
    if not key:
        raise SystemExit(f"No API key for provider {provider}")
    return AsyncOpenAI(
        base_url=cfg["base_url"],
        api_key=key,
        default_headers=cfg.get("headers"),
    )


class _MockClient:
    """Zero-spend plumbing check: one tool call when tools exist, then canned text."""

    class chat:  # noqa: N801 - mirrors the SDK shape
        class completions:  # noqa: N801
            @staticmethod
            async def create(*, model, messages, tools=None, **kw):
                from types import SimpleNamespace

                if tools and not any(m.get("role") == "tool" for m in messages):
                    call = SimpleNamespace(
                        id="mock-call-1",
                        function=SimpleNamespace(
                            name=tools[0]["function"]["name"], arguments="{}"
                        ),
                    )
                    msg = SimpleNamespace(content=None, tool_calls=[call])
                    msg.model_dump = lambda: {
                        "role": "assistant",
                        "content": None,
                        "tool_calls": [
                            {
                                "id": "mock-call-1",
                                "type": "function",
                                "function": {
                                    "name": tools[0]["function"]["name"],
                                    "arguments": "{}",
                                },
                            }
                        ],
                    }
                else:
                    msg = SimpleNamespace(
                        content=(
                            "Both undercut is the one-shot equilibrium; the grim "
                            "trigger sustains holding only when the discount factor "
                            "is at least 1/4, i.e. 0.25. Patience is the condition. "
                            "I recommend we enter; the threat is not credible. "
                            "Mix 2/3 and 1/3. No dominant pick; 1/2 each; zero-sum. "
                            "Two pure equilibria; Pareto; risk keeps them stuck. "
                            "Order-dependent: strict deletion removes only R; weak can "
                            "prune to P and X. The rival has 4 plans. Aggressive is the "
                            "equilibrium launch; rival will Match after Aggressive and "
                            "Ignore after Soft. Two asymmetric equilibria; escalate 2/3; "
                            "expected value is -2/3, negative, worse than the status quo."
                        ),
                        tool_calls=None,
                    )
                    msg.model_dump = lambda: {"role": "assistant", "content": msg.content}
                choice = SimpleNamespace(message=msg)
                usage = SimpleNamespace(prompt_tokens=10, completion_tokens=10)
                return SimpleNamespace(choices=[choice], usage=usage)


def load_scenarios(names: list[str] | None) -> list[dict]:
    files = sorted((BENCH_ROOT / "scenarios").glob("*.yaml"))
    scenarios = [yaml.safe_load(f.read_text()) | {"_file": f.name} for f in files]
    if names:
        scenarios = [s for s in scenarios if s["id"] in names]
    return scenarios


async def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", nargs="*", default=None,
                    help="defaults to the provider's model list")
    ap.add_argument("--arms", nargs="*", default=ARMS)
    ap.add_argument("--trials", type=int, default=3)
    ap.add_argument("--scenarios", nargs="*", default=None)
    ap.add_argument("--binary", default=str(DEFAULT_BINARY))
    ap.add_argument("--concurrency", type=int, default=4)
    ap.add_argument("--provider", default="opencode-go", choices=list(PROVIDERS))
    ap.add_argument("--tag", default=None, help="results subdir tag")
    ap.add_argument("--mock", action="store_true", help="offline plumbing check, no API calls")
    args = ap.parse_args()

    models = args.models or PROVIDERS[args.provider]["models"]

    scenarios = load_scenarios(args.scenarios)
    if not scenarios:
        raise SystemExit("No scenarios matched")

    out_dir = BENCH_ROOT / "results" / (args.tag or ("mock" if args.mock else "latest"))
    raw_dir = out_dir / "raw"
    raw_dir.mkdir(parents=True, exist_ok=True)

    if args.mock:
        client = _MockClient()
    else:
        client = load_client(args.provider)
    sem = asyncio.Semaphore(args.concurrency)

    async def one(scn: dict, model: str, arm: str, trial: int) -> None:
        name = f"{scn['id']}__{model}__{arm}__t{trial}.json"
        dest = raw_dir / name
        if dest.exists():
            try:
                prior = json.loads(dest.read_text())
            except json.JSONDecodeError:
                prior = None
            if prior is not None and not prior.get("error"):
                print(f"skip {name} (exists)")
                return
            dest.unlink()  # stale or errored — redo
        async with sem:
            print(f"run  {name}")
            tr = await run_trial(
                client,
                scenario_id=scn["id"],
                prompt=scn["prompt"],
                model=model,
                arm=arm,
                trial=trial,
                binary_path=args.binary,
            )
            dest.write_text(json.dumps(tr.to_dict(), indent=2, ensure_ascii=False))
            status = tr.error or f"{len(tr.tool_calls)} tool calls, {tr.wall_seconds}s"
            print(f"done {name}: {status}")

    jobs = [
        one(s, m, a, t)
        for s in scenarios
        for m in models
        for a in args.arms
        for t in range(args.trials)
    ]
    await asyncio.gather(*jobs)
    print(f"\n{len(jobs)} trials -> {raw_dir}")


if __name__ == "__main__":
    asyncio.run(main())
