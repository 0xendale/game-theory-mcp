"""Single benchmark trial: one conversation, bare or MCP-armed."""

from __future__ import annotations

import asyncio
import json
import time
from dataclasses import dataclass, field
from typing import Any

import openai
from openai import AsyncOpenAI

from .mcp_bridge import McpBridge

MAX_ROUNDS = 12
RETRIES = 6

# gpt-5.x are served through Copilot's Responses API, not chat/completions.
RESPONSES_MODELS = {"gpt-5-mini", "gpt-5.4-mini"}

SYSTEM_BARE = (
    "You are a sharp strategy consultant. Solve the client's problem analytically. "
    "Show your derivation, give concrete numbers where the problem supports them, "
    "and end with a clear recommendation."
)
SYSTEM_MCP = (
    "You are a sharp strategy consultant with access to exact game-theory "
    "computation tools (validate_game, solve_dominance, solve_pure_nash, "
    "solve_mixed_nash, solve_backward_induction, convert_form, verify_equilibrium, "
    "analyze_payoff_structure, analyze_repeated_game). Formalize the client's "
    "situation as a game and USE THE TOOLS for every computation - never do "
    "equilibrium arithmetic by hand. Then explain the result in the client's "
    "language and end with a clear recommendation."
)


@dataclass
class ToolCallRecord:
    round: int
    name: str
    arguments: dict[str, Any]
    result_preview: str
    is_error: bool


@dataclass
class Transcript:
    scenario_id: str
    model: str
    arm: str
    trial: int
    final_text: str = ""
    tool_calls: list[ToolCallRecord] = field(default_factory=list)
    input_tokens: int = 0
    output_tokens: int = 0
    wall_seconds: float = 0.0
    error: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "scenario_id": self.scenario_id,
            "model": self.model,
            "arm": self.arm,
            "trial": self.trial,
            "final_text": self.final_text,
            "tool_calls": [vars(tc) for tc in self.tool_calls],
            "usage": {"input_tokens": self.input_tokens, "output_tokens": self.output_tokens},
            "wall_seconds": self.wall_seconds,
            "error": self.error,
        }


def _args(raw: str | None) -> dict[str, Any]:
    if not raw:
        return {}
    try:
        parsed = json.loads(raw)
        return parsed if isinstance(parsed, dict) else {"_value": parsed}
    except json.JSONDecodeError:
        return {"_unparseable": raw}


async def _create_with_retry(fn, kwargs: dict[str, Any]):
    for attempt in range(RETRIES):
        try:
            return await fn(**kwargs)
        except openai.RateLimitError as exc:
            if attempt == RETRIES - 1:
                raise
            # Respect the provider's Retry-After when present; otherwise back
            # off exponentially. Transient limits (Copilot, Gemini) clear in
            # seconds-to-minutes; a hard quota (opencode-go free tier) does not,
            # and every attempt here costs wall time but nothing else.
            delay = 5 * (2**attempt)
            retry_after = getattr(exc, "response", None)
            if retry_after is not None:
                try:
                    header = retry_after.headers.get("Retry-After")
                    if header:
                        delay = max(delay, int(float(header)) + 2)
                except (ValueError, TypeError):
                    pass
            await asyncio.sleep(delay)
    raise AssertionError("unreachable")


async def run_trial(
    client: AsyncOpenAI,
    *,
    scenario_id: str,
    prompt: str,
    model: str,
    arm: str,
    trial: int,
    binary_path: str,
) -> Transcript:
    tr = Transcript(scenario_id=scenario_id, model=model, arm=arm, trial=trial)
    started = time.monotonic()
    try:
        if arm == "mcp":
            async with McpBridge(binary_path) as bridge:
                await _loop(client, tr, prompt, model, bridge)
        else:
            await _loop(client, tr, prompt, model, None)
    except Exception as exc:  # record, don't crash the matrix
        tr.error = f"{type(exc).__name__}: {exc}"
    tr.wall_seconds = round(time.monotonic() - started, 2)
    return tr


async def _loop(
    client: AsyncOpenAI,
    tr: Transcript,
    prompt: str,
    model: str,
    bridge: McpBridge | None,
) -> None:
    system = SYSTEM_MCP if bridge else SYSTEM_BARE
    messages: list[dict[str, Any]] = [
        {"role": "system", "content": system},
        {"role": "user", "content": prompt},
    ]
    tools = bridge.tools_openai if bridge else None

    if model in RESPONSES_MODELS:
        await _loop_responses(client, tr, messages, model, bridge)
        return

    for round_idx in range(MAX_ROUNDS):
        kwargs: dict[str, Any] = {"model": model, "messages": messages, "temperature": 0}
        if tools:
            kwargs["tools"] = tools
        resp = await _create_with_retry(client.chat.completions.create, kwargs)
        if resp.usage:
            tr.input_tokens += resp.usage.prompt_tokens or 0
            tr.output_tokens += resp.usage.completion_tokens or 0
        msg = resp.choices[0].message

        if not msg.tool_calls:
            tr.final_text = msg.content or ""
            return

        # exclude_none: a tool-call-only assistant turn has content=None, which
        # some providers (Gemini) reject outright on echo.
        messages.append(msg.model_dump(exclude_none=True))
        for tc in msg.tool_calls:
            arguments = _args(tc.function.arguments)
            assert bridge is not None
            result = await bridge.call(tc.function.name, arguments)
            tr.tool_calls.append(
                ToolCallRecord(
                    round=round_idx,
                    name=tc.function.name,
                    arguments=arguments,
                    result_preview=result.text[:400],
                    is_error=result.is_error,
                )
            )
            messages.append(
                {"role": "tool", "tool_call_id": tc.id, "content": result.text}
            )

    tr.final_text = "(stopped: max tool rounds reached)"


def _responses_tools(openai_tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Chat-shaped tools -> Responses API shape: name moves to the top level."""
    return [
        {
            "type": "function",
            "name": t["function"]["name"],
            "description": t["function"]["description"],
            "parameters": t["function"]["parameters"],
        }
        for t in openai_tools
    ]


async def _loop_responses(
    client: AsyncOpenAI,
    tr: Transcript,
    initial_messages: list[dict[str, Any]],
    model: str,
    bridge: McpBridge | None,
) -> None:
    system = next(m["content"] for m in initial_messages if m["role"] == "system")
    prompt = next(m["content"] for m in initial_messages if m["role"] == "user")
    tools = _responses_tools(bridge.tools_openai) if bridge else None
    items: list[dict[str, Any]] = [
        {"type": "message", "role": "user", "content": [{"type": "input_text", "text": prompt}]}
    ]

    for round_idx in range(MAX_ROUNDS):
        kwargs: dict[str, Any] = {
            "model": model,
            "instructions": system,
            "input": items,
        }
        if tools:
            kwargs["tools"] = tools
        resp = await _create_with_retry(client.responses.create, kwargs)
        if resp.usage:
            tr.input_tokens += resp.usage.input_tokens or 0
            tr.output_tokens += resp.usage.output_tokens or 0

        calls = [it for it in resp.output if it.type == "function_call"]
        if not calls:
            tr.final_text = resp.output_text or ""
            return

        for call in calls:
            arguments = _args(call.arguments)
            assert bridge is not None
            result = await bridge.call(call.name, arguments)
            tr.tool_calls.append(
                ToolCallRecord(
                    round=round_idx,
                    name=call.name,
                    arguments=arguments,
                    result_preview=result.text[:400],
                    is_error=result.is_error,
                )
            )
            items.append(
                {
                    "type": "function_call",
                    "call_id": call.call_id,
                    "name": call.name,
                    "arguments": call.arguments,
                }
            )
            items.append(
                {"type": "function_call_output", "call_id": call.call_id, "output": result.text}
            )

    tr.final_text = "(stopped: max tool rounds reached)"
