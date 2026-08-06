# Report Fixtures

Files ending in `_report.json` are immutable historical analyzer inputs. They
preserve previously sampled reports for native Rust regression coverage, so
retired path strings such as `src/git_slop/organization.py` inside the JSON are
sample data rather than current `git-slop` dependencies.

The `.txt` files and `relationship_focused_plan.json` are output goldens. They
may be updated when an intentional native renderer or planner change alters the
expected presentation or projection.

`health_folder_guidance_report.json` is a focused synthetic schema 4 input for
folder guidance. Its paired `health_folder_guidance.md` golden locks the exact
files-only, tokens-only, and combined-trigger Markdown projection, including
bounded descendant evidence and copyable next commands.

Do not hand-edit historical input reports to satisfy dependency scans. Add or
regenerate an input only when the report contract intentionally changes.
