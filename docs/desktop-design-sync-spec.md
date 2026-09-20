# Desktop Design Synchronization

**Status: implemented; human acceptance pending.** The user approved implementation
on 2026-09-21 after resolving all twelve interview choices. This specification
consolidates the current reference and retained native contracts. Automated
verification and human acceptance are recorded separately below.

## Confirmed scope and precedence

- Review the whole Desktop design, without first narrowing the discussion to the
  latest Library and Personal Lists changes. Include Home, Sidebar and account
  controls, Library, Personal Lists, movie/series/episode details, playback
  presentations, Settings, and the shared desktop components and interaction states.
- Preserve accepted native product semantics where Paper differs. A new design
  reference does not supersede an accepted native decision. Record any future
  explicitly approved exception separately before changing that contract.
- The reference is the current [Paper Desktop page](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0)
  and its accompanying design specifications. Use the
  [desktop component library](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/4-0)
  for shared controls and theme counterparts. Mobile, Pad/Duo and TV are outside
  this desktop review.
- Reference proposals and archived explorations remain identified as such; whole
  Desktop coverage does not itself accept them as implementation targets.

Native terminology remains in [CONTEXT.md](../CONTEXT.md). Existing accepted
presentation and interaction rules remain in the [design system](design-system.md),
[Sidebar specification](sidebar-design-spec.md), and
[Home Hero specification](home-hero-design-spec.md).

## Native contracts retained

- Favorites belong to the server user; Watchlist belongs to this device and Profile
  Scope. Their membership and watched state remain independent
  ([ADR 0032](adr/0032-separate-server-favorites-and-local-watchlist.md)).
- Profile Switch validates the target before replacing the active connection
  ([ADR 0033](adr/0033-validate-before-active-profile-handoff.md)).
- Desktop Intro Skipper uses Automatic/Manual with persistent series preferences.
  Automatic skips remain silent. Paper's session-only switch and automatic-skip
  Undo do not replace this contract
  ([ADR 0045](adr/0045-desktop-series-intro-skipper-preferences.md)).
- Detail audio/subtitle popovers describe source tracks and remain read-only;
  Direct Playback and active-playback track controls retain their existing roles.
- Settings retain explicit Save/Enter for text drafts and immediate persistence
  for the existing switches and selectors. Paper's blanket autosave note does not
  change these semantics.
- Existing native player visibility, responsive layout, account isolation, and
  confirmed-write behavior remain in force. Static examples do not establish real
  media facts, server capabilities, or successful mutations.

## Desktop coverage

| Area | Synchronization target and retained native contract |
| --- | --- |
| Shared controls, Sidebar and account | Migrate filled primary actions through shared tokens/Catalogs in both themes. Retain Sidebar search, compact rail, account quick-menu structure and lifecycle safeguards. |
| Home | Use the current Home reference for shared control treatment; retain the 440px foreground, real Featured Item/selection-rail behavior and independent Continue Watching entries. |
| Library Grid/List | Add explicit play/resume controls and current-episode resume presentation using native playback targets. Match the two-row filter/tools hierarchy and compact hover actions. Preserve query state, paging and independent Grid/List scroll positions. |
| Personal Lists | Add play/resume controls, Watchlist/Favorites removal Undo and local History removal/Undo. Match the current three-section composition, episode metadata slots and removal states. |
| Details | Cover movie, series and episode pages, including shared controls and wrapped source specifications. Preserve actual episode identity, series-only Next Up, read-only source-track popovers, full media information and parent-series navigation. |
| Footer Player Bar | Retain the already docked composition, three logical zones and queue/audio/subtitle popovers. Compare current component styling and hit regions. Preserve Stop, Show video and responsive reflow. The independent five-state v2 board is supplementary, not blanket acceptance of new behavior. |
| Embedded player | Migrate applicable filled-action styling while retaining the accepted scrims, instantaneous region visibility, independent Information, playback controls, responsive layout and Intro Skipper policy. |
| Control-Only | Apply shared action styling and current external-controller visual details within the existing composition and window constraints. Do not adopt the reference's example dimensions as a replacement App Mode contract. |
| Settings | Cover all eight categories and shared controls. Retain existing settings, the constrained two-column modal, explicit field saves, real status, validation and persistence errors. |
| Common states | Cover loading, empty, failure, keyboard focus, long content, unavailable media and dismissal. The current component-library recovery examples guide presentation while native state transitions remain authoritative. |

The 2026-09-21 reference component index now labels the six-width PlayBar board as
historical. The native design system explicitly accepted its control arrangement
while retaining native scrims and responsive behavior. The reference's reclassification
does not revoke that accepted native layout contract.

## Confirmed interaction targets

### Shared filled-action palette

Migrate all desktop filled primary actions through shared tokens and Catalog
styles, including existing consumers, rather than adding per-page overrides.
Dark and light themes use the current Paper action palette: default `#4F46E5`,
hover `#5B55E7`, pressed `#4338CA`, with the existing white on-action content.
Brand/progress primary, selected surfaces, switch tracks, semantic status colors,
neutral controls and glass treatments retain their separate roles.

Preserve disabled behavior and visible keyboard focus. Native instantaneous
control-state changes and the existing motion contract remain authoritative;
the new palette does not adopt the reference's generic transition timings.
Use both English and Simplified Chinese resources for new interface text, keeping
media titles and metadata as provided by the server.

### Explicit playback controls

Library Grid/List and Personal Lists gain explicit play/resume controls. Artwork,
titles and row areas outside action controls continue to open the corresponding
detail page. Playback, collection actions and row navigation must not trigger one
another.

Reuse native movie/episode playback and series Next Up selection. This decision
does not introduce Paper's client-side first-unwatched or all-watched series replay
fallback. The displayed episode and remaining time must describe the same real
target that the action will play; missing target data must not become fabricated
progress or an arbitrary first episode.

### Collection removal and Undo

Adopt removal feedback and Undo for Watchlist and Favorites, and add Watch History
removal with Undo. Removing an item affects only the chosen collection. History
removal hides its local presentation without clearing Played, resume position or
server history. Collection changes retain native confirmed-write behavior: a
failed write cannot produce a success message or a confirmed count change.

Each successful removal has its own Undo action and 8-second feedback timer. Stack
the notices rather than replacing an earlier removal's Undo with a newer one, and
pause the corresponding timer while its notice is hovered or focused. Undo acts
only on that removal; consecutive removals are not one implicit batch. Ordinary
page navigation retains the notice and its remaining Undo time. Successful account
switching, Disconnect, Sign Out of the active account, and application exit end
those Undo opportunities without reversing successful removals. A cancelled or
failed profile handoff that retains the original account does not itself discard
its notices. Undo is unavailable while an account handoff blocks scoped writes.

Restore focus appropriately after keyboard removal and Undo without stealing focus
for pointer actions. If navigation has removed the originating item, do not
navigate back or focus an unrelated item to simulate restoration. Intervening
same-item mutations must not allow an old Undo to overwrite a newer confirmed
local intention. Notice overflow follows the confirmed queue policy below.

Only a confirmed successful removal creates Undo feedback. While its Undo write
is pending, hold that notice and prevent duplicate submission; failures retain an
honest retry state rather than reporting success or discarding the operation.
Successful Undo restores only the affected collection. Watchlist restores the
original record/order when no newer re-addition exists; server-backed lists retain
their current canonical ordering and real data. Late results cannot update a
different account or resurrect an expired notice.

Persist desktop History hiding on this device within the originating Profile
Scope. It survives application restart. New viewing makes the corresponding item
visible again; an ordinary refresh of unchanged History does not. Undo also
restores visibility. This choice does not synchronize hiding to other devices or
remove the underlying server progress.

The accepted new-viewing signals are a reliably newer server last-played timestamp
than the observation stored for the hidden item, including updates from another
client, or a successful new local Playback Session for that item. Repeated progress
reports, pause/resume within the same session, refresh, failed playback starts,
and Played flag changes alone do not restore visibility. Compare server values
with server values, not with the device's wall clock.

When the server timestamp is missing or cannot be compared with the hidden
observation, retain hiding until an accepted signal or Undo is available. A first
usable timestamp can establish an observation baseline without being presented as
proof of a newer viewing event. This is deliberately conservative: external viewing
that does not advance usable server metadata may remain hidden until a later
qualifying observation or successful local playback.

Both providers expose an optional full last-played timestamp in the existing
History item model. A strictly later server value can be compared with the value
observed when hiding, but there is no viewing-event identity or guarantee about
timestamp update frequency. Missing/equal timestamps, changed resume position and
manually changed Played flags cannot establish a new playback session on their
own. The local wall clock is not a substitute for the server observation baseline.

The ownership and persistence decision is recorded in
[ADR 0046](adr/0046-local-desktop-history-visibility.md). It accepts an observable
server-record update, not proof that an external client started a particular
Playback Session.

The evolved shared History store retains compatibility with item-only records.
Desktop APIs observe last-played baselines and new local playback; Android keeps
its existing item-only read/hide behavior. Watchlist Undo restores exact saved
records and original order while retaining newer re-additions. Favorites Undo
uses a confirmed server write and is not an atomic rollback of remote state.
Independent removal notices coexist with the existing dismiss-only error Toast.

### Docked footer

The footer Player Bar already fills the main content width and docks to its bottom
edge, with zero outer margin, zero outer corner radius and a top hairline. Retain
that composition: current code and the Accepted Paper Home Composition already
agree, so docking itself requires no structural change. This target does not
restyle the embedded video control layer or the separate Control-Only composition.

Retain native playback commands, including Stop and Show video where applicable,
active-playback track selection, volume, queue, and responsive reflow. Place content
and removal feedback so the docked bar cannot cover the last reachable actions.

## Reference-derived presentation details

These details come from the current Desktop and desktop-component references and
are included in the final specification review; they do not add new media-server
capabilities or change the native contracts retained above.

- Library places the mutually exclusive viewing-state control and independent
  Favorites filter together, followed by a separate row with count at the left and
  sorting plus Grid/List controls at the right. Sorting changes order; changing
  view preserves the query. Keep at least 40px interactive regions for these tools.
- Wide Library lists use a 220px current-episode resume lane with episode identity
  and remaining time, replacing the generic percentage-only presentation. Its
  play/resume action is an inline icon/text control rather than a filled primary
  button. Retain native narrow-row reflow and access to all actions.
- Poster/landscape hover and keyboard-action presentation uses a 48px circular
  play control and 40px corner collection controls. Tooltips belong to the specific
  hovered/focused control, not every action on the card simultaneously. Collection
  actions must not bubble into playback or navigation.
- Episode cards give series identity, season/episode identity, remaining time or
  recency, and the optional episode title distinct space. Keep episode numbers
  and time readable; truncate flexible titles rather than the whole metadata row.
  Full names remain available through detail and accessible descriptions.
- Personal Lists uses the current 36px/40px page title, 20px/28px section titles,
  36px wide-window side insets, 28px section separation and 14px title-to-content
  spacing. Keep the existing responsive shelves and scrollbar clearance. Counts
  belong to their individual sections without a duplicated page-header summary.
- A series' progress represents the resolved episode's resume position, not a
  made-up whole-series percentage. Loading, no progress, failed progress lookup,
  unavailable media and no native Next Up target remain distinct. Show retry for
  failures and disable unresolved/unavailable playback without disabling valid
  detail or collection actions. No arbitrary first-episode fallback is introduced.
- Current footer/component references use a 4px resting timeline and a 6px hovered
  timeline within a 40px interaction region. Preserve real buffering/markers and
  the existing seek contract; narrow layouts can grow rather than enforce the
  reference's approximately 65px wide-layout height on every window.
- Existing native focus, reduced-motion behavior, text wrapping, modal dismissal,
  unavailable-state truthfulness and viewport-bound image demand remain in force.
  Cover both themes and languages; example counts, addresses, media specifications
  and artwork are not production fixtures.

## Counts and notice capacity

The server History total includes locally hidden rows. Show an exact visible total
only when it is known; otherwise omit the total and keep ordinary pagination and
loading feedback. Do not relabel the server total, subtract an unchecked set of
stored hidden IDs, or scan all History solely to populate a count badge. An unknown
total is not zero. Favorites and Watchlist retain their own truthful counts.

Display at most three independent Undo notices together, reducing that number when
the available height requires it. Queue additional notices in order. Each notice
starts its own eight-second opportunity when first presented, not while waiting
behind other notices; hover/focus pauses only its corresponding timer. If capacity
changes move a notice back into the queue, retain its remaining time. No new notice
can overwrite another removal's operation or silently merge it into a batch.

Fit the stack above the docked footer and keep each visible action reachable.
Do not transfer keyboard focus when a notice is added or when a queued notice
becomes visible. Explicit dismissal ends that notice's Undo opportunity; it does
not reverse the removal. Account invalidation and application exit retire queued
notices as well as displayed ones.

## Acceptance scenarios

The automated checks below cover the state, persistence, provider and interaction
boundaries. These end-to-end scenarios remain the human acceptance checklist;
neither mocked server responses nor a startup smoke establish live-server or
visual acceptance.

- Open Library Grid, List and Personal Lists with the same movie, episode and
  series. Detail navigation, direct playback and collection actions remain
  independent; labels and episode progress match the actual native playback target.
  Exercise absent Next Up, missing progress, failed loading and unavailable media.
- Remove entries from Watchlist, Favorites and History. Verify each collection's
  ownership, confirmed success/failure feedback, unchanged other collections and
  unchanged server playback progress when hiding History. Undo restores only its
  operation and remains safe after a same-item re-addition or repeated activation.
- Remove at least four entries quickly. No Undo opportunity is overwritten or
  started while queued. Exercise hover/focus pauses, manual dismissal, partial
  expiry, retry, resizing and ordinary navigation without losing operation identity.
- Switch accounts, Disconnect, cancel a profile switch, and restart the app.
  Successful connection changes retire old-account Undo; cancelled handoff keeps
  eligible notices. Persisted hides remain scoped and late results cannot change
  another account's data or reintroduce retired notices.
- Re-read an unchanged hidden record, observe a strictly newer server timestamp,
  and start a successful new local Playback Session. Cover missing, equal and
  incomparable timestamps, failed starts, progress-only updates and existing
  item-only stored hides. Preserve Android behavior when shared storage evolves.
- Exercise partial History pages containing hidden items. Pagination still reaches
  all visible records; counts are either justified visible totals or absent,
  without replacing unknown with zero or mixing account/query revisions.
- Human review covers all Desktop surfaces in both themes and interface languages,
  wide/narrow/short windows, keyboard-only use and long text. Compare shared action
  colors, reference spacing, hover controls, metadata lanes, docked footer and Undo
  clearance while retaining the native player, Settings and account exceptions.

## Delivery boundary

Product terms are recorded in [CONTEXT.md](../CONTEXT.md); the consequential History
ownership and persistence choice is in [ADR 0046](adr/0046-local-desktop-history-visibility.md).
The desktop implementation includes shared filled-action styles, Library and
Personal Lists playback targets and layout, independent collection-removal Undo,
persisted desktop History visibility, truthful History pagination and the footer
timeline treatment. Existing native contracts above remain in effect.

Verification on 2026-09-21 follows the cross-crate and native-smoke tiers in the
[validation policy](agents/validation.md):

- Focused core, SDK and media-server tests passed, including timestamp comparison,
  receipt generations, failed persistence and both providers' native Next Up seam.
- The final desktop test run passed: 373 application tests, 159 UI widget tests and
  10 embedded-host tests. Coverage includes independent card actions, truthful
  playback targets, Undo queue timing, account transitions, focus preservation,
  application-mode request isolation and ordered History hide/playback/Undo.
- Focused Clippy, `bun run check` and
  `xvfb-run -a bun run task iced run --smoke` passed. The smoke proves startup and
  first-frame completion only.
- `bun run task rust test` passed after the separately owned SDK browse notification
  changes settled: 1,377 tests passed, zero failed. One lifecycle child-process
  helper is marked ignored because its parent lifecycle tests launch it explicitly.
- Documentation links and consistency were checked. The acceptance scenarios above
  remain pending human review in both themes and languages, including real server
  playback and persistence across application restarts.
