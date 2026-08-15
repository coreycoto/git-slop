# Worked Example

This runnable, repository-neutral flow treats Git Slop as evidence, not a
refactoring oracle. Replace `src/parser.rs` with a tracked path from your report.

## 1. Inspect readiness and estimate cost

```bash
git slop doctor
git slop find --estimate-only
```

If the repository has not adopted Git Slop yet, the ordinary first scan is
automatically Git-private and does not create `.slop/` adoption files:

```bash
git slop find
git slop health
git slop doctor
git slop html
```

For durable use, initialize, review, and commit only the two adoption files:

```bash
git slop init
git diff -- .slop/config.yaml .slop/.gitignore
git add .slop/config.yaml .slop/.gitignore
git commit -m "Adopt Git Slop"
```

## 2. Produce one current report

```bash
git slop find
git slop doctor --require-current
```

`find` prints a concise health table and a scan receipt. It writes the complete
machine report, dashboard, and immutable run snapshot once; downstream commands
read that evidence without rescoring it.

## 3. Select and explain one path

```bash
git slop list policy-failures --top 10
git slop list interventions --top 10
git slop list observations --top 10
git slop list health-findings --top 10
git slop show src/parser.rs
git slop explain --path src/parser.rs
```

Suppose the report says `src/parser.rs` exceeds the context budget and has a
supported relationship to `tests/parser_test.rs`. Before changing anything,
apply domain judgment:

- the public parsing behavior must remain unchanged;
- a generated parser table is observation-only and must not be hand-edited;
- the cited test is necessary but may not be sufficient;
- an out-of-scope dependency means the slice should stop and be regenerated.

## 4. Save the reviewed before-state

```bash
git slop baseline ensure --name parser-before
git slop baseline inspect --name parser-before
```

If readiness fails, resolve the listed blocker rather than weakening the
comparison by default. A clean report from complete evidence should baseline
without `--allow-dirty` or `--allow-incomplete-evidence`.

## 5. Generate a bounded proposal

```bash
git slop plan --path src/parser.rs
```

Keep only a slice with a concrete objective, named in-scope paths, explicit
exclusions, discovered verification commands, measurable outcomes, and a stop
condition. The output includes copyable baseline and rerun commands. It remains
preview-only: it does not edit code or create backlog work.

## 6. Make and verify the smallest change

Run the repository-native commands named by the plan. A typical Rust example is:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --test parser_test
```

If the change needs an out-of-scope file, stop. Either abandon it or regenerate
the plan with the newly reviewed boundary.

## 7. Rescan and compare

```bash
git slop find
git slop doctor --require-current
git slop compare \
  --baseline parser-before \
  --head .slop/latest/report.json \
  --detail full \
  --fail-on-regression
```

Accept the change only when repository verification passes, the intended
evidence improves or becomes more explainable, and native comparison reports no
unrelated regression. A lower score alone is not proof that the code is better.

## 8. Close the loop

If the movement is intentional and becomes the new reviewed reference:

```bash
git slop baseline ensure --name parser-before --replace
```

Otherwise revert the scoped code change. To remove the local baseline later:

```bash
git slop baseline remove --name parser-before
git slop baseline remove --name parser-before --yes
```

The detector supplies reproducible evidence. The maintainer owns correctness,
scope, acceptance, and the decision to change code.
