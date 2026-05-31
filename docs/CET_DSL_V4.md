# CET DSL v4

CET v4 adds a production query layer above the native C engine.

Supported sequence syntax:

```text
A,B,C
A+,B,C
A?,B,C
A{1,3},B,C
(A|B),C
```

Supported semantic layers:

- `where`: event payload predicates.
- `absence`: negative event checks between trend start/end.
- `relations`: same/different constraints over path event attributes.
- `score`: deterministic scoring knobs.

The native engine executes the sequence superset. The Python runtime validates
features that are intentionally outside the native hot path. This keeps the C
kernel fast and auditable while allowing richer security semantics.
