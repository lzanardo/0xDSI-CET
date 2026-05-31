"""Attack Chain Compiler for CET.

This layer lets 0xDSI authors describe detection logic as an attack-chain
object and compile it into the governed CET query registry format.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class AttackChainStep:
    event_type: str
    alias: str | None = None
    min_repeats: int = 1
    max_repeats: int | None = 1

    def to_pattern_token(self) -> str:
        token = self.event_type
        if self.min_repeats == 0 and self.max_repeats == 1:
            return f"{token}?"
        if self.min_repeats == 1 and self.max_repeats is None:
            return f"{token}+"
        if self.max_repeats is not None and (self.min_repeats, self.max_repeats) != (1, 1):
            return f"{token}{{{self.min_repeats},{self.max_repeats}}}"
        if self.max_repeats is None and self.min_repeats != 1:
            return f"{token}{{{self.min_repeats},}}"
        return token


@dataclass(frozen=True)
class AttackChain:
    chain_id: str
    version: str
    name: str
    steps: tuple[AttackChainStep, ...]
    within_ms: int
    slide_ms: int
    severity: str = "medium"
    where: tuple[dict[str, Any], ...] = ()
    relations: tuple[dict[str, Any], ...] = ()
    absence: tuple[dict[str, Any], ...] = ()
    score: dict[str, Any] = field(default_factory=dict)
    mitre: tuple[str, ...] = ()
    tags: tuple[str, ...] = ()

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "AttackChain":
        steps = []
        for s in d.get("steps", []):
            steps.append(AttackChainStep(
                event_type=str(s["event_type"]),
                alias=s.get("alias"),
                min_repeats=int(s.get("min_repeats", 1)),
                max_repeats=(None if ("max_repeats" in s and s.get("max_repeats") is None) else int(s.get("max_repeats", 1))),
            ))
        return cls(
            chain_id=str(d.get("chain_id") or d.get("query_id") or d["name"]),
            version=str(d.get("version", "v1")),
            name=str(d.get("name") or d.get("chain_id") or d.get("query_id")),
            steps=tuple(steps),
            within_ms=int(d["within_ms"]),
            slide_ms=int(d.get("slide_ms", d["within_ms"])),
            severity=str(d.get("severity", "medium")),
            where=tuple(d.get("where", [])),
            relations=tuple(d.get("relations", [])),
            absence=tuple(d.get("absence", [])),
            score=dict(d.get("score", {})),
            mitre=tuple(str(x) for x in d.get("mitre", [])),
            tags=tuple(str(x) for x in d.get("tags", [])),
        )

    def to_query_spec_dict(self) -> dict[str, Any]:
        pattern = ",".join(s.to_pattern_token() for s in self.steps)
        tags = list(dict.fromkeys([*self.tags, "attack_chain", *self.mitre]))
        score = dict(self.score)
        if self.mitre:
            score.setdefault("mitre", list(self.mitre))
        return {
            "query_id": self.chain_id,
            "version": self.version,
            "name": self.name,
            "pattern": pattern,
            "within_ms": self.within_ms,
            "slide_ms": self.slide_ms,
            "severity": self.severity,
            "where": list(self.where),
            "relations": list(self.relations),
            "absence": list(self.absence),
            "score": score,
            "tags": tags,
            "enabled": True,
        }


def compile_attack_chain(document: dict[str, Any]) -> dict[str, Any]:
    """Compile one chain or a document containing `attack_chains` to registry JSON."""
    if "attack_chains" in document:
        return {"queries": [AttackChain.from_dict(x).to_query_spec_dict() for x in document["attack_chains"]]}
    return {"queries": [AttackChain.from_dict(document).to_query_spec_dict()]}
