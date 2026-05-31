"""Query registry loader for CET v4."""
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable
import json

from .dsl import QuerySpec, parse_query_spec


def _load_yaml_if_available(path: Path) -> Any:
    try:
        import yaml  # type: ignore
    except Exception as exc:  # pragma: no cover - optional dependency
        raise RuntimeError("YAML registry requires PyYAML; use JSON or install oxdsi-cet[yaml]") from exc
    return yaml.safe_load(path.read_text())


def load_registry_document(path: str | Path) -> dict[str, Any]:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    if p.suffix.lower() in {".yaml", ".yml"}:
        data = _load_yaml_if_available(p)
    else:
        data = json.loads(text)
    if isinstance(data, list):
        data = {"queries": data}
    if not isinstance(data, dict) or "queries" not in data:
        raise ValueError("query registry must contain a top-level 'queries' list")
    return data


@dataclass(frozen=True)
class QueryRegistry:
    queries: tuple[QuerySpec, ...]

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "QueryRegistry":
        return cls(tuple(parse_query_spec(q) for q in data.get("queries", [])))

    @classmethod
    def from_file(cls, path: str | Path) -> "QueryRegistry":
        return cls.from_dict(load_registry_document(path))

    @classmethod
    def from_iterable(cls, queries: Iterable[dict[str, Any] | QuerySpec]) -> "QueryRegistry":
        out = [q if isinstance(q, QuerySpec) else parse_query_spec(q) for q in queries]
        return cls(tuple(out))

    def enabled(self) -> list[QuerySpec]:
        return [q for q in self.queries if q.enabled]

    def get(self, query_id: str, version: str | None = None) -> QuerySpec:
        matches = [q for q in self.queries if q.query_id == query_id and (version is None or q.version == version)]
        if not matches:
            raise KeyError(f"query not found: {query_id} {version or ''}".strip())
        return sorted(matches, key=lambda q: q.version)[-1]

    def validate_unique_versions(self) -> None:
        seen: set[tuple[str, str]] = set()
        for q in self.queries:
            key = (q.query_id, q.version)
            if key in seen:
                raise ValueError(f"duplicate query version: {key}")
            seen.add(key)
