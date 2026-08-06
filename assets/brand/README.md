# Git Slop brand mark

Git Slop uses one semantic mark rather than a general-purpose icon family. It
adapts Primer Octicons' Git branch topology and replaces the original check
with a plain shield. The solid terminal and shield-center dot create a visual
rhyme between repository state and repository protection.

The mark is intentionally monochrome. Use the dark master on light surfaces
and the inverse master on dark surfaces; do not add semantic color to the
terminal, shield, or center dot.

## Assets

| Asset | Purpose |
| --- | --- |
| `git-slop.svg` | Canonical `#24292f` vector master for light surfaces |
| `git-slop-inverse.svg` | Exact `#f0f6fc` inverse for dark surfaces |
| `git-slop-16.png` | Optically checked transparent export at 16 px |
| `git-slop-24.png` | Optically checked transparent export at 24 px |
| `git-slop-512.png` | Transparent raster master for surfaces that do not accept SVG |
| `../../plugins/git-slop/assets/git-slop.svg` | Exact plugin-package mirror of the canonical SVG |

The repository README selects the light or dark asset for the viewer's color
scheme and keeps the visible `🧑‍💻🤖🫟 Git Slop` title intact.

GitHub Actions Marketplace does not accept a custom SVG in `action.yml`.
GitHub's metadata contract permits a supported Feather icon and preset badge
color, so the listing uses `shield` with `purple` as the closest native
treatment.

## Usage

When `Git Slop` is visible next to the mark, treat the image as decorative:

```html
<img src="assets/brand/git-slop.svg" alt="" width="24" height="24">
```

When the mark is the only visible identification, give it an accessible name:

```html
<img src="assets/brand/git-slop.svg" alt="Git Slop" width="24" height="24">
```

Keep the mark monochrome, preserve its proportions and clear space, and use a
visible text label anywhere its meaning would otherwise be ambiguous.

## Reproducing raster exports

From the repository root with Inkscape installed:

```bash
for size in 16 24 512; do
  inkscape assets/brand/git-slop.svg \
    --export-background-opacity=0 \
    --export-width="$size" \
    --export-height="$size" \
    --export-filename="assets/brand/git-slop-$size.png"
done
```

## Provenance and license

The branch topology is adapted from Primer Octicons'
[`git-branch-check-24`](https://primer.style/octicons/icon/git-branch-check-24/).
The replacement shield was redrawn at the optical weight of that icon, informed
by Primer's [`shield-24`](https://primer.style/octicons/icon/shield-24/). The
derivative mark is used under the Octicons MIT license, preserved in
[`LICENSE-OCTICONS`](LICENSE-OCTICONS).

No GitHub logo, Octocat, or GitHub wordmark is incorporated. The surrounding
Git Slop repository remains licensed under the repository's MIT license.
