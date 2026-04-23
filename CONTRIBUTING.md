# Contributing

Keep `git-slop` repo-facing and product-facing.

Validation and dogfood may use private or external repositories, but committed
repo content must not name them. Do not check external repo names into:

- docs
- fixtures
- tests
- snapshots
- examples

When validation history is worth keeping, rewrite it with neutral role-based
labels such as `local repo`, `mature validation repo`, `smaller application
repo`, or `consumer toolkit repo`.
