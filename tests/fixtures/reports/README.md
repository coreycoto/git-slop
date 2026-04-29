# Report Fixtures

These JSON files are historical analyzer fixtures. They preserve previously sampled repository reports exactly enough for regression coverage, so legacy path strings such as `tests/unit/agent_tools/...` inside the JSON are sample data rather than current `git-slop` dependencies.

Do not hand-edit fixture payloads to satisfy dependency scans. Regenerate them from the analyzer when the expected report shape changes.
