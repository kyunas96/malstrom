# Spec: Filter project list by filename

## Goal
Let the user type text into a search box to narrow the `.als` project list (`AlsProjectList.tsx`) to entries whose `name` contains that text (case-insensitive substring match).

## Where it plugs in
- **State**: one `useState<string>('')` in `AlsProjectList.tsx`, named `nameFilter` — same pattern as the component's other filter state.
- **Filtering logic**: add one more `useMemo` step in `useFilteredProjects.ts`, chained alongside the existing scope/tag filters, before pin ordering:
  ```ts
  const nameFiltered = useMemo(
    () => nameFilter.trim()
      ? scopeFiltered.filter(p => p.name.toLowerCase().includes(nameFilter.trim().toLowerCase()))
      : scopeFiltered,
    [scopeFiltered, nameFilter]
  );
  ```
  Feed `nameFiltered` into the next stage instead of `scopeFiltered`.
- **UI**: a plain `<input type="text" placeholder="Filter by name…">` in `.scales-header-controls`, next to `ScaleFilterBar`/`ScaleFilterControls`. No new component file needed — inline in `AlsProjectList.tsx` unless it grows beyond a few lines, in which case pull it into a `NameFilterInput.tsx` alongside the sibling filter components.

## Behavior
- Empty input → no filtering (current behavior unchanged).
- Match is substring, case-insensitive, against `name` only (not `path`).
- Combines with existing scale/tag filters (AND) — it's just another stage in the same chain.
- No debounce: list sizes here are small enough (virtualized, in-memory) that filtering on every keystroke is fine.

## Out of scope
- Fuzzy matching, regex, or path search — add only if requested.
- Highlighting matched substring in `ProjectNameCell` — cosmetic, separate task if wanted.

## Testing
Manual: type a partial filename, confirm only matching rows remain and virtualization/row count updates correctly; clear input, confirm full list returns.
