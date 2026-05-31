"""Output sinks for Standing Trend Runtime."""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
import json
import urllib.request

from .runtime import StandingTrendEvent, trend_event_to_json


@dataclass
class MemoryTrendSink:
    events: list[StandingTrendEvent] = field(default_factory=list)

    def emit(self, event: StandingTrendEvent) -> None:
        self.events.append(event)

    def as_dicts(self) -> list[dict[str, Any]]:
        return [e.as_dict() for e in self.events]


@dataclass
class JsonlTrendSink:
    path: str

    def emit(self, event: StandingTrendEvent) -> None:
        p = Path(self.path)
        p.parent.mkdir(parents=True, exist_ok=True)
        with p.open("a", encoding="utf-8") as fh:
            fh.write(trend_event_to_json(event) + "\n")


@dataclass
class ZeroBusTrendSink:
    """HTTP-style ZeroBus sink.

    The ZeroBus connector is transport-pluggable because the final ZeroBus wire
    protocol is environment-specific. This sink supports the common case of a
    JSON POST endpoint and keeps the contract isolated from the standing runtime.
    """

    endpoint: str
    token: str | None = None
    timeout_seconds: float = 10.0
    headers: dict[str, str] = field(default_factory=dict)

    def emit(self, event: StandingTrendEvent) -> None:
        body = trend_event_to_json(event).encode("utf-8")
        headers = {"Content-Type": "application/json", **self.headers}
        if self.token:
            headers.setdefault("Authorization", f"Bearer {self.token}")
        req = urllib.request.Request(self.endpoint, data=body, headers=headers, method="POST")
        with urllib.request.urlopen(req, timeout=self.timeout_seconds) as resp:  # nosec: runtime-configured endpoint
            resp.read()
