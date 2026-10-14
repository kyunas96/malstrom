# Mashup: constraints and stages

## Starting idea

Make a new `.als` from two existing projects: Group A holds everything from
TrackA.als, and Group B holds everything from TrackB.als. Live's browser can
already do this by hand. What Malstrom adds is the scale overlay on the
project list, which shows which pairs are worth mashing.

## Where v0 stands

`build_mashup` ([mashup.rs](../src-tauri/src/als/mashup.rs)) uses project A
as the base. The new set takes A's tempo, time signature and master track.
Return tracks come from both projects (see "Return tracks" below). The tracks you pick from both projects are spliced in. B's IDs are
shifted so they don't collide with A's, and B's groove links are removed.
Live opens the result, so the low-level XML work mostly works already. What's
left are mostly musical decisions.

## The constraints

### Things a set can only have one of

These can't sit inside a group, so "Group A holds all of TrackA.als" can
never be literally true.

- **Tempo.** One value per set, plus optional tempo automation on the master.
  Someone has to choose A's tempo, B's, or something in between. MIDI and
  warped audio follow the tempo. Unwarped audio doesn't, so it drifts out of
  sync.
- **Time signature.** Also set for the whole song. Mixing 4/4 with 6/8 still
  runs, but it won't sound musical.
- **Master track.** Only one. B's master effects (limiter, glue compressor)
  are dropped, so B can sound different afterwards.
- **Return tracks.** Groups can't contain returns, and in Live every track has
  exactly one send knob per return. If A has 2 returns and B has 3, B's tracks
  have 3 sends that need to line up with A's 2. The options are:
  - (a) Drop B's returns and their sends. B loses its reverb and delay.
  - (b) Add B's returns after A's. That makes 5 returns, so every track needs
    5 send slots, with zeros added to the ones it doesn't use. This is the
    honest option and it's doable.
  - (c) Merge returns that look alike. This is a guess and unreliable.

  **Done: (b).** Each track's mixer sends are matched to the returns by
  position, so A's tracks get B's-return-count "off" sends added after their
  own, and B's tracks get A's-return-count added before theirs. `SendsPre`
  (one pre/post flag per return) gets B's entries too. Confirmed in Live
  with 1-12-26 (2 returns) + 10-12-26 (4): B's tracks still send to B's
  reverbs.

### Things that have to line up across the set

- **Scenes and clip slots.** Every track has one clip slot per scene. If A has
  8 scenes and B has 16, the slots don't match. You'd have to pad A's tracks
  or cut B's.
- **Arrangement timeline.** Both projects start at bar 1, so they play over
  each other. Do you offset B, or line them up from a locator? Locators and
  loop region also come from A only.
- **Key.** Once you've picked two projects in different keys, MIDI can be
  transposed easily, and audio clips have their own transpose setting.

### Resources that belong to the project, not the track

- **Samples.** Paths that point inside B's project folder break when the new
  set lives in a different folder. You'd need to copy B's files over, or use
  absolute paths.
- **Groove pool.** v0 strips B's grooves. That could be merged later with the
  same ID shifting.
- **Third-party plugins and Live version.** Both projects must open on the
  same machine. The `als_upgrade` CLI exists because of the version problem.

### Practical limits

- Two whole projects means twice the CPU load.
- The two projects' loudness levels probably won't match.
- Live allows groups inside groups, so wrapping A's existing groups inside
  "Group A" works.

## Stages

Every constraint above is something that can't simply be copied into the new
project, so the feature has to make a decision for each one. The stages are
about which of those decisions Malstrom makes for you, and in what order to
build them. Each one builds on the one before.

### Stage 1: Stack (the starting idea, with its gaps handled)

You pick TrackA.als and TrackB.als. Malstrom creates one new project:

- **Group A** holds A's tracks and **Group B** holds B's tracks.
- **Tempo:** Malstrom asks which one to use, with A's as the default.
- **Return tracks:** both projects keep theirs. If A has 2 and B has 3, the
  new project has 5. Each group keeps sending to its own returns, so nothing
  loses its reverb.
- **Session scenes:** if A has 8 rows and B has 16, A gets 8 empty rows added
  so everything lines up.

What you get is both songs fully intact in one project, ready for you to
start muting and arranging. It does nothing musical for you. It just saves
you the manual work of combining them correctly.

### Stage 2: Align (making them fit together musically)

Malstrom already knows each project's key from its scale inference, so it
could:

- shift B's notes into A's key
- say something like "B is 85 BPM, A is 170. Play B at double time instead of
  stretching it."
- copy B's sample files into the new project so none of them go missing

This stage is the "mashup" part. Stage 1 is only the plumbing.

### Stage 3: Pick (choosing parts, not whole songs)

Instead of all of A plus all of B, you tick "A's drums and bass, plus B's
vocal and lead." This is the checklist already planned in the README. The
current backend can already pull individual tracks. What's missing is the UI.

## Recommendation

Build Stage 1 first. It turns the starting idea into something that actually
works. Stage 2 is where Malstrom adds something Live's browser can't do, and
Stage 3 is a nicer way to choose the parts.

## Open questions

- How does v0 handle projects with different scene counts? Check this against
  real files.
