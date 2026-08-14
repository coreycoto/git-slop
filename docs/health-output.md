# Health Output

`git slop health` renders an existing schema-5 report without rerunning the
detector or rewriting persisted artifacts.

## Formats

- `markdown`: the repository-health dashboard persisted as `health.md` by
  `find`.
- `text`: the concise interactive terminal view and default.
- `github`: bounded GitHub workflow-command annotations.
- `json`: an automation payload containing the additive health section.

Every format writes to standard output. Only `find` writes
`.slop/latest/health.md`; `health` never changes that file.

## Evidence concepts

The dashboard keeps three related concepts separate:

- **Context/load bands** (`compact`, `healthy`, `warning`, and
  `budget_exceeded`) describe how much `agent_context` content must be loaded.
  File bands use file tokens; folder bands use direct child-file counts and
  direct tokens.
- **Maintenance-pressure evidence** is the stable `slop_score` and `slop_band`
  derived from deterministic load, history, and coordination signals. It is
  neither an overall quality score nor another name for a context/load band.
- **Finding severity** (`notice`, `warning`, or `error`) is the rendered
  review priority. It stays the same in Markdown and GitHub annotations; policy
  mode does not promote or demote it.

## Folder explanations

Every surfaced warning or budget-exceeded folder states the exact boundary
that produced its displayed band. For example, `19 direct files > 17 healthy
ceiling` identifies the observation and configured boundary. When direct files
and direct tokens both cross the relevant ceiling, both clauses are shown.

The row includes a copyable command such as
`git-slop explain --path src/` (`--path .` for the repository root) and one
highest-ranked recursive `agent_context` descendant. That descendant is chosen
by descending maintenance-pressure score, then descending tokens, then
ascending path.

## Number rendering

Markdown number formatting is locale-independent. Integer counts and token
totals use comma grouping; non-integral percentiles use comma grouping and two
decimal places; concentration and profile shares use one decimal place plus
`%`; and maintenance-pressure scores use one decimal place. JSON retains
numeric values and types.
