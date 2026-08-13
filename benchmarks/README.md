# Benchmarks: LLM with vs without game-theory-mcp

Measures what the MCP server adds to a cheap model's strategic reasoning.
Each scenario is asked twice — bare model, and the same model with the nine
tools over stdio — and scored against exact answer keys computed by the server
itself.

## Setup

```sh
uv venv .venv && uv pip install openai mcp pyyaml pydantic
cargo build --release -p game-theory-mcp
```

Providers, chosen with `--provider`:

| Provider | Endpoint | Models |
|---|---|---|
| `copilot` | `api.githubcopilot.com` (GitHub Copilot key from `github-copilot.access`) | `gpt-5-mini`, `gpt-5.4-mini` |
| `opencode-go` | `opencode.ai/zen/go/v1` (free tier, 5-hour quota) | `deepseek-v4-flash` |
| `gemini` | `generativelanguage.googleapis.com/v1beta/openai/` | `gemini-3.6-flash` |

`gpt-5.x` are served by Copilot's Responses API, not `chat/completions`, so the
harness switches protocols per model (`RESPONSES_MODELS` in `agent.py`). Other
providers have independent quotas; use the one with headroom. Keys come from
`~/.local/share/opencode/auth.json` unless the matching
`<PROVIDER>_API_KEY` env var is set.

## Run

```sh
.venv/bin/python -m harness.runner --mock            # plumbing check, no spend
.venv/bin/python -m harness.runner --tag v0.1.0 --provider copilot    # gpt-5-mini + gpt-5.4-mini
.venv/bin/python -m harness.runner --tag v0.1.0      # opencode-go (deepseek-v4-flash)
.venv/bin/python -m harness.runner --tag v0.1.0 --provider gemini   # gemini-3.6-flash
.venv/bin/python -m harness.score  --tag v0.1.0      # score -> summary.md
```

Defaults: model `deepseek-v4-flash` (or the provider's model), arms `bare`,
`mcp`; 3 trials per cell; concurrency 4. All overridable via `--models`,
`--arms`, `--trials`, `--scenarios`, `--concurrency`. Existing raw files are
skipped, so reruns are incremental — a run interrupted by a quota error resumes
exactly where it stopped.

## Layout

- `scenarios/*.yaml` — persona prompt + `expected_tools` + `assertions`.
  Assertions are regex sets; a trial scores the fraction matched in the final
  answer. Answer keys were computed with `compute_keys.py` against the release
  binary, never hand-derived.
- `harness/` — `mcp_bridge` (stdio client), `agent` (chat loop), `runner`
  (matrix), `score` (summary).
- `results/<tag>/raw/*.json` — full transcripts (gitignored).
  `results/<tag>/summary.md|json` — committed.

## Arms

- `bare`: consultant system prompt, no tools.
- `mcp`: same prompt shape plus the nine tools, instructed to use them for
  every computation. One fresh MCP session per trial.

Temperature 0 everywhere.
