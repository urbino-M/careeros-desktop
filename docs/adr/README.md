# Architecture Decision Records

Use an ADR only for an important decision that constrains future architecture. ADRs preserve why a durable choice was made; they are not a diary of ordinary implementation work.

## When to Write an ADR

Write one when a decision introduces a durable architectural constraint and materially affects areas such as:

- subsystem boundaries or ownership;
- a new persisted-data compatibility or migration strategy that future changes must follow;
- external integrations, security, or secrets;
- runtime/process architecture;
- a cross-cutting contract that future changes must preserve;
- adoption or removal of a foundational dependency.

Do not write an ADR for:

- a normal bug fix;
- copy, styling, icons, or local UI behavior;
- a small code change with no durable architectural consequence;
- refactoring that preserves existing boundaries and contracts.

## Naming

Use the next four-digit sequence and a short kebab-case title:

```text
0001-example-decision.md
```

Do not reuse an existing number. Supersede an accepted decision with a new ADR. The only historical edit normally allowed is changing the old ADR's status and adding its `Superseded by` link; do not rewrite its context or decision.

## Template

```markdown
# ADR-XXXX: Title

## Status

Accepted | Superseded | Deprecated

Supersedes: ADR-XXXX (optional)

Superseded by: ADR-XXXX (optional)

## Context

What problem, constraints, and forces required a durable decision?

## Decision

What was chosen and which boundaries must future work preserve?

## Alternatives

Which credible options were considered, and why were they not chosen?

## Consequences

What benefits, costs, risks, migration needs, and follow-up obligations result?
```

Keep ADRs focused on the decision. Put current module locations and operational change routes in `docs/ARCHITECTURE.md` or a feature handoff, where they can evolve without rewriting decision history.
