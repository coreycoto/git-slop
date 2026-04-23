# Label Palette Contract

Keep the backlog label vocabulary small, semantically legible, and visually
distinct.

Default rules:

- labels should support filtering and automation, not replace the issue taxonomy
- prefer GitHub defaults when they already cover the semantic role cleanly
- keep repo-managed labels intentionally few and deterministic
- let the checked-in label palette manifest define repo-owned colors and names

The sync workflow should:

- validate the checked-in palette first
- preview the resulting delta before apply
- manage repo-owned labels only
- avoid deleting labels or restyling GitHub defaults unless the workflow
  contract explicitly says otherwise
