"""MCP stdio bridge: spawn game-theory-mcp, expose its tools as OpenAI function tools."""

from __future__ import annotations

import copy
from contextlib import AsyncExitStack
from dataclasses import dataclass
from typing import Any

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


@dataclass
class ToolCallResult:
    text: str
    is_error: bool


def _deref_schema(schema: dict[str, Any]) -> dict[str, Any]:
    """Make a server schema acceptable to strict OpenAI-style providers.

    - Inlines local $refs so providers without $defs support (Gemini) accept it.
    - Drops a top-level oneOf/anyOf/allOf when the node already declares
      ``type: "object"`` — the union is redundant there (the variants are the
      same shapes the properties describe) and providers like Copilot reject
      any union keyword at the top level of a function schema. Nested unions
      survive; only the root is affected.
    """

    defs = schema.get("$defs", {})
    out = copy.deepcopy(schema)
    out.pop("$defs", None)
    for kw in ("oneOf", "anyOf", "allOf"):
        out.pop(kw, None)

    def walk(node: Any) -> Any:
        if isinstance(node, dict):
            ref = node.get("$ref")
            if isinstance(ref, str) and ref.startswith("#/$defs/"):
                node = copy.deepcopy(defs[ref.removeprefix("#/$defs/")])
            return {k: walk(v) for k, v in node.items() if k != "$defs"}
        if isinstance(node, list):
            return [walk(v) for v in node]
        return node

    return walk(out)


class McpBridge:
    """One stdio MCP session per benchmark trial (fresh state, deterministic)."""

    def __init__(self, binary_path: str):
        self._params = StdioServerParameters(command=binary_path, args=[])
        self._stack = AsyncExitStack()
        self.session: ClientSession | None = None
        self.tools_openai: list[dict[str, Any]] = []

    async def __aenter__(self) -> "McpBridge":
        try:
            read, write = await self._stack.enter_async_context(stdio_client(self._params))
            self.session = await self._stack.enter_async_context(ClientSession(read, write))
            await self.session.initialize()
            listed = await self.session.list_tools()
        except BaseException:
            # A half-entered stack leaves the subprocess pump task orphaned and
            # hangs asyncio.run's shutdown cancellation.
            await self._stack.aclose()
            raise
        self._raw_tools = listed.tools
        self.tools_openai = [
            {
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": (t.description or "")[:1024],
                    "parameters": _deref_schema(t.input_schema),
                },
            }
            for t in listed.tools
        ]
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self._stack.aclose()

    async def call(self, name: str, arguments: dict[str, Any]) -> ToolCallResult:
        assert self.session is not None
        result = await self.session.call_tool(name, arguments)
        parts = [
            getattr(c, "text", str(c)) for c in result.content
        ]
        return ToolCallResult(text="\n".join(parts), is_error=bool(result.is_error))
