# Manual test: filename filter

Companion to [filename-filter-spec.md](filename-filter-spec.md). Run these against
a root folder with enough `.als` projects that names vary and a few overlap in
substring (e.g. "Track 1", "Track 1 Remix", "Track 2").

## Basic filtering
- [ ] Type a substring that matches one project name (any case) → only that
      row remains.
- [ ] Type a substring that matches several project names → all matches
      remain, no others.
- [ ] Type mixed case (e.g. "TRACK") → matches regardless of the project
      name's actual case.
- [ ] Type a substring that matches no project → list is empty (not an
      error state).
- [ ] Clear the input → full (unfiltered-by-name) list returns.
- [ ] Type only whitespace → treated as empty, full list shown.

## Scope
- [ ] Filter matches against the project name only — typing a substring
      that only appears in the folder path (not the name) returns no
      results.

## Combines with existing filters (AND)
- [ ] With a scale tag active, add a name filter that further narrows the
      tag-filtered set → result is the intersection, not a fresh unfiltered
      search.
- [ ] With a name filter active, toggle Common/Exotic scales or adjust the
      coverage slider → name filter stays applied.
- [ ] With a name filter active, add or remove a scale tag → name filter
      stays applied and result set stays the intersection.

## Pinned rows
- [ ] Pin a project (via tag) with no name filter, then type a name filter
      that excludes it → confirm it disappears from the pinned section
      (existing behavior for tag/scope filters, not new — just confirming
      the name filter doesn't do anything different).
- [ ] Clear the name filter → the pinned project reappears above the
      divider.

## Interaction / performance
- [ ] Type quickly across a large scanned folder → list updates on every
      keystroke with no visible lag or dropped input (no debounce by
      design).
- [ ] Row virtualization stays correct as the row count shrinks/grows while
      typing — no leftover blank rows, no scroll position glitches.
- [ ] Row count / divider index update correctly as filtered set changes.

## Regression
- [ ] With the filter empty, all other list behavior (sorting via pin,
      scale pull actions, tag toggling) is unchanged from before this
      feature.
