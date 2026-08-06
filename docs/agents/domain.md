# Domain Docs

How engineering skills should consume this repository's domain documentation.

## Before exploring

- Read `CONTEXT.md` at the repository root when it exists.
- Read ADRs under `docs/adr/` that touch the area being explored.
- If these files do not exist, proceed silently. The domain-modeling workflow creates them only when terms or decisions are resolved.

## Layout

This is a single-context repository:

```text
/
├── CONTEXT.md
├── docs/adr/
└── src/
```

## Vocabulary

Use the glossary's vocabulary in issue titles, specifications, tests, and code. Avoid synonyms that the glossary explicitly rejects. If a needed domain concept is missing, reconsider the term or capture the gap through domain modeling.

## Decisions

If proposed work contradicts an existing ADR, surface the conflict rather than silently overriding the recorded decision.
