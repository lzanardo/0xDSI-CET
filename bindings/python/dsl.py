"""CET DSL v4 compiler and validators.

The native C engine remains intentionally small and fast. This module adds the
production query layer used by 0xDSI:

- sequence syntax: A, A+, A?, A{m,n}, (A|B)
- event predicates over payload fields
- absence/negative conditions
- relation constraints such as same_user/same_host
- deterministic compilation to the native sequence subset

The compiler is conservative. If a query uses semantics the C engine cannot
represent directly, the runtime executes a native superset and validates the
extra semantics in Python before emitting a trend.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable
import re

_NATIVE_UNSUPPORTED = {"optional", "bounded", "or", "absence", "relations", "where"}


def _split_top_level(pattern: str) -> list[str]:
    tokens: list[str] = []
    buf: list[str] = []
    paren = 0
    brace = 0
    for ch in pattern:
        if ch == "(" and brace == 0:
            paren += 1
        elif ch == ")" and brace == 0:
            paren = max(0, paren - 1)
        elif ch == "{" and paren == 0:
            brace += 1
        elif ch == "}" and paren == 0:
            brace = max(0, brace - 1)
        if ch == "," and paren == 0 and brace == 0:
            tok = "".join(buf).strip()
            if tok:
                tokens.append(tok)
            buf = []
        else:
            buf.append(ch)
    tok = "".join(buf).strip()
    if tok:
        tokens.append(tok)
    return tokens


@dataclass(frozen=True)
class EventStep:
    raw: str
    aliases: tuple[str, ...]
    min_repeats: int = 1
    max_repeats: int | None = 1

    @property
    def canonical(self) -> str:
        return self.aliases[0]

    @property
    def optional(self) -> bool:
        return self.min_repeats == 0

    @property
    def kleene_plus(self) -> bool:
        return self.min_repeats == 1 and self.max_repeats is None

    @property
    def bounded(self) -> bool:
        return self.max_repeats not in (1, None) or self.min_repeats not in (0, 1)

    @property
    def has_or(self) -> bool:
        return len(self.aliases) > 1

    def native_token(self) -> str:
        # Native fast path supports exact event type and Kleene plus only.
        return f"{self.canonical}+" if self.kleene_plus else self.canonical

    def referenced_types(self) -> set[str]:
        return set(self.aliases)


def parse_step(token: str) -> EventStep:
    raw = token.strip()
    min_rep = 1
    max_rep: int | None = 1
    body = raw

    bounded = re.search(r"\{\s*(\d+)\s*(?:,\s*(\d*)\s*)?\}$", body)
    if bounded:
        min_rep = int(bounded.group(1))
        upper = bounded.group(2)
        max_rep = None if upper == "" else int(upper) if upper is not None else min_rep
        body = body[: bounded.start()].strip()
    elif body.endswith("+"):
        min_rep = 1
        max_rep = None
        body = body[:-1].strip()
    elif body.endswith("?"):
        min_rep = 0
        max_rep = 1
        body = body[:-1].strip()

    if body.startswith("(") and body.endswith(")"):
        aliases = tuple(x.strip() for x in body[1:-1].split("|") if x.strip())
    else:
        aliases = (body,)
    if not aliases or any(not a for a in aliases):
        raise ValueError(f"invalid CET step: {token!r}")
    if max_rep is not None and max_rep < min_rep:
        raise ValueError(f"invalid repeat bounds in {token!r}")
    return EventStep(raw=raw, aliases=aliases, min_repeats=min_rep, max_repeats=max_rep)


def parse_pattern(pattern: str) -> list[EventStep]:
    steps = [parse_step(tok) for tok in _split_top_level(pattern)]
    if not steps:
        raise ValueError("empty CET pattern")
    return steps


def _get_field(event: dict[str, Any], field: str) -> Any:
    if field in event:
        return event[field]
    attrs = event.get("attributes") or event.get("payload") or {}
    cur: Any = attrs
    for part in field.split("."):
        if isinstance(cur, dict) and part in cur:
            cur = cur[part]
        else:
            return None
    return cur


@dataclass(frozen=True)
class FieldPredicate:
    field: str
    op: str
    value: Any = None
    event_type: str | None = None
    scope: str = "event"

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "FieldPredicate":
        return cls(
            field=str(d["field"]),
            op=str(d.get("op", "eq")).lower(),
            value=d.get("value"),
            event_type=d.get("event_type"),
            scope=str(d.get("scope", "event")),
        )

    def evaluate(self, event: dict[str, Any]) -> bool:
        if self.event_type and str(event.get("event_type")) != self.event_type:
            return True
        actual = _get_field(event, self.field)
        op = self.op
        expected = self.value
        if op in {"exists", "present"}:
            return actual is not None
        if op in {"not_exists", "absent"}:
            return actual is None
        if op in {"eq", "=="}:
            return actual == expected
        if op in {"ne", "!="}:
            return actual != expected
        if op == "in":
            return actual in (expected or [])
        if op == "not_in":
            return actual not in (expected or [])
        if op == "contains":
            return actual is not None and str(expected) in str(actual)
        if op == "regex":
            return actual is not None and re.search(str(expected), str(actual)) is not None
        try:
            a = float(actual)
            b = float(expected)
        except (TypeError, ValueError):
            return False
        if op in {"gt", ">"}:
            return a > b
        if op in {"gte", ">="}:
            return a >= b
        if op in {"lt", "<"}:
            return a < b
        if op in {"lte", "<="}:
            return a <= b
        raise ValueError(f"unsupported predicate op: {self.op}")


@dataclass(frozen=True)
class AbsenceSpec:
    event_type: str
    within_ms: int | None = None
    field_predicates: tuple[FieldPredicate, ...] = ()

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "AbsenceSpec":
        return cls(
            event_type=str(d["event_type"]),
            within_ms=d.get("within_ms"),
            field_predicates=tuple(FieldPredicate.from_dict(x) for x in d.get("where", [])),
        )

    def matches(self, event: dict[str, Any]) -> bool:
        if str(event.get("event_type")) != self.event_type:
            return False
        return all(p.evaluate(event) for p in self.field_predicates)


@dataclass(frozen=True)
class RelationConstraint:
    name: str
    field: str
    steps: tuple[int, ...] = ()
    op: str = "same"

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "RelationConstraint":
        return cls(
            name=str(d.get("name") or d.get("relation") or d.get("field")),
            field=str(d["field"]),
            steps=tuple(int(x) for x in d.get("steps", [])),
            op=str(d.get("op", "same")),
        )

    def validate(self, path_events: list[dict[str, Any]]) -> bool:
        if not path_events:
            return True
        selected = [path_events[i] for i in self.steps if 0 <= i < len(path_events)] if self.steps else path_events
        values = [_get_field(e, self.field) for e in selected]
        values = [v for v in values if v is not None]
        if len(values) <= 1:
            return True
        if self.op == "same":
            return len(set(values)) == 1
        if self.op == "different":
            return len(set(values)) == len(values)
        raise ValueError(f"unsupported relation op: {self.op}")


@dataclass(frozen=True)
class QuerySpec:
    query_id: str
    version: str
    pattern: str
    within_ms: int
    slide_ms: int
    name: str | None = None
    severity: str = "medium"
    enabled: bool = True
    where: tuple[FieldPredicate, ...] = ()
    absence: tuple[AbsenceSpec, ...] = ()
    relations: tuple[RelationConstraint, ...] = ()
    score: dict[str, Any] = field(default_factory=dict)
    tags: tuple[str, ...] = ()

    @property
    def steps(self) -> list[EventStep]:
        return parse_pattern(self.pattern)

    def referenced_event_types(self) -> set[str]:
        out: set[str] = set()
        for step in self.steps:
            out.update(step.referenced_types())
        for item in self.absence:
            out.add(item.event_type)
        return out

    def unsupported_native_features(self) -> set[str]:
        features: set[str] = set()
        for step in self.steps:
            if step.optional:
                features.add("optional")
            if step.bounded:
                features.add("bounded")
            if step.has_or:
                features.add("or")
        if self.absence:
            features.add("absence")
        if self.relations:
            features.add("relations")
        if self.where:
            features.add("where")
        return features

    def supports_native_fast_path(self) -> bool:
        return not self.unsupported_native_features()

    def native_pattern(self) -> str:
        return ",".join(step.native_token() for step in self.steps if not step.optional)

    def native_name(self) -> str:
        return f"{self.query_id}:{self.version}"


def parse_query_spec(d: dict[str, Any]) -> QuerySpec:
    return QuerySpec(
        query_id=str(d.get("query_id") or d.get("id") or d.get("name")),
        version=str(d.get("version", "v1")),
        name=d.get("name"),
        pattern=str(d["pattern"]),
        within_ms=int(d["within_ms"]),
        slide_ms=int(d.get("slide_ms", d["within_ms"])),
        severity=str(d.get("severity", "medium")),
        enabled=bool(d.get("enabled", True)),
        where=tuple(FieldPredicate.from_dict(x) for x in d.get("where", [])),
        absence=tuple(AbsenceSpec.from_dict(x) for x in d.get("absence", [])),
        relations=tuple(RelationConstraint.from_dict(x) for x in d.get("relations", [])),
        score=dict(d.get("score", {})),
        tags=tuple(str(x) for x in d.get("tags", [])),
    )


def filter_events_for_query(query: QuerySpec, events: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    refs = query.referenced_event_types()
    out: list[dict[str, Any]] = []
    for e in events:
        if refs and str(e.get("event_type")) not in refs:
            continue
        if not all(p.evaluate(e) for p in query.where):
            continue
        out.append(e)
    return out


def validate_absence(query: QuerySpec, path_events: list[dict[str, Any]], all_events: Iterable[dict[str, Any]]) -> bool:
    if not query.absence or not path_events:
        return True
    start = min(int(e.get("event_time_ms", 0)) for e in path_events)
    end = max(int(e.get("event_time_ms", 0)) for e in path_events)
    partition = path_events[0].get("partition_key")
    path_ids = {e.get("event_id") for e in path_events}
    for e in all_events:
        if e.get("event_id") in path_ids:
            continue
        if partition is not None and e.get("partition_key") != partition:
            continue
        ts = int(e.get("event_time_ms", 0))
        if ts < start or ts > end:
            continue
        for absence in query.absence:
            if absence.within_ms is not None and ts - start > int(absence.within_ms):
                continue
            if absence.matches(e):
                return False
    return True


def validate_relations(query: QuerySpec, path_events: list[dict[str, Any]]) -> bool:
    return all(r.validate(path_events) for r in query.relations)


def validate_path(query: QuerySpec, path_events: list[dict[str, Any]], all_events: Iterable[dict[str, Any]]) -> bool:
    if not path_events:
        return False
    if not validate_absence(query, path_events, all_events):
        return False
    if not validate_relations(query, path_events):
        return False
    return True
