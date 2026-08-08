# Worked Example

This neutral example treats Git Slop as evidence, not a refactoring oracle.

1. Run `git slop find` and `git slop show src/parser.rs`.
2. Inspect `git slop explain --path src/parser.rs`. Suppose the file exceeds the context budget and has a supported relationship to `tests/parser_test.rs`.
3. Apply human judgment: the boundary is real, the generated parser table must remain untouched, and public parsing behavior must not change.
4. Generate `git slop plan --path src/parser.rs`. Keep only a slice with a concrete objective, bounded paths, discovered test command, measurable outcome, rerun command, and abandonment condition.
5. Make the smallest change and run the repository's tests.
6. Rescan to a new immutable run and compare: `git slop compare --base .slop/runs/<before>/report.json --head .slop/latest/report.json`.
7. Accept the change only if repository verification passes and the intended evidence improves without unrelated regressions. Otherwise roll it back or abandon the slice.

The detector supplies reproducible evidence. The maintainer owns correctness, scope, and the decision to change code.
