"""ZeroBus connector for 0xDSI-CET.

ZeroBus does not currently have public protocol documentation, so this connector
is intentionally transport-pluggable. It supports:

- file/jsonl mode for deterministic tests and local replay;
- http mode for simple polling endpoints returning JSON arrays or NDJSON;
- mock mode for unit tests and embedded usage.

A production ZeroBus driver can subclass `ZeroBusClient` or add a new transport
without changing the downstream CET/Spark contracts.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Iterator
import json
import os
import time
import urllib.request

from .base import ConnectorConfig, normalize_connector_event


@dataclass(frozen=True)
class ZeroBusConfig(ConnectorConfig):
    transport: str = "file"  # file | http | mock
    endpoint: str | None = None
    token: str | None = None
    file_path: str | None = None
    timeout_seconds: float = 15.0
    headers: dict[str, str] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "ZeroBusConfig":
        base = dict(d)
        return cls(
            source_name=str(base.get("source_name", "zerobus")),
            target_table=str(base.get("target_table", "main.cet.cet_events")),
            batch_size=int(base.get("batch_size", 1000)),
            max_messages=base.get("max_messages"),
            poll_interval_seconds=float(base.get("poll_interval_seconds", 1.0)),
            attributes=dict(base.get("attributes", {})),
            transport=str(base.get("transport", "file")),
            endpoint=base.get("endpoint") or os.getenv("ZEROBUS_ENDPOINT"),
            token=base.get("token") or os.getenv("ZEROBUS_TOKEN"),
            file_path=base.get("file_path") or os.getenv("ZEROBUS_FILE"),
            timeout_seconds=float(base.get("timeout_seconds", 15.0)),
            headers=dict(base.get("headers", {})),
        )


class ZeroBusClient:
    def __init__(self, config: ZeroBusConfig, mock_events: Iterable[dict[str, Any]] | None = None):
        self.config = config
        self._mock_events = list(mock_events or [])

    def _iter_file(self) -> Iterator[dict[str, Any]]:
        if not self.config.file_path:
            raise ValueError("ZeroBus file transport requires file_path")
        path = Path(self.config.file_path)
        with path.open("r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                yield json.loads(line)

    def _iter_http_once(self) -> Iterator[dict[str, Any]]:
        if not self.config.endpoint:
            raise ValueError("ZeroBus http transport requires endpoint")
        headers = dict(self.config.headers)
        if self.config.token:
            headers.setdefault("Authorization", f"Bearer {self.config.token}")
        req = urllib.request.Request(self.config.endpoint, headers=headers)
        with urllib.request.urlopen(req, timeout=self.config.timeout_seconds) as resp:  # nosec: connector runtime config controls endpoint
            body = resp.read().decode("utf-8")
        text = body.strip()
        if not text:
            return
        if text.startswith("["):
            for item in json.loads(text):
                yield item
        else:
            for line in text.splitlines():
                line = line.strip()
                if line:
                    yield json.loads(line)

    def poll(self) -> Iterator[dict[str, Any]]:
        transport = self.config.transport.lower()
        if transport == "file":
            yield from self._iter_file()
        elif transport == "http":
            yield from self._iter_http_once()
        elif transport == "mock":
            yield from self._mock_events
        else:
            raise ValueError(f"unsupported ZeroBus transport: {self.config.transport}")

    def poll_normalized(self) -> Iterator[dict[str, Any]]:
        emitted = 0
        for raw in self.poll():
            yield normalize_connector_event(raw, source_name=self.config.source_name).as_dict()
            emitted += 1
            if self.config.max_messages is not None and emitted >= int(self.config.max_messages):
                break

    def batches(self) -> Iterator[list[dict[str, Any]]]:
        batch: list[dict[str, Any]] = []
        for event in self.poll_normalized():
            batch.append(event)
            if len(batch) >= self.config.batch_size:
                yield batch
                batch = []
                if self.config.poll_interval_seconds > 0:
                    time.sleep(self.config.poll_interval_seconds)
        if batch:
            yield batch


def zero_bus_events(config: dict[str, Any] | ZeroBusConfig, mock_events: Iterable[dict[str, Any]] | None = None) -> list[dict[str, Any]]:
    cfg = config if isinstance(config, ZeroBusConfig) else ZeroBusConfig.from_dict(config)
    return list(ZeroBusClient(cfg, mock_events=mock_events).poll_normalized())
