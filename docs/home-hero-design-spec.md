# JellyPilot Video Home Hero and Resume Interaction Specification

**Status: Paper Home synchronization implemented; human visual acceptance pending.** The foreground Hero is fixed at 440 logical pixels and the original Backdrop scales independently without cropping. Code-level verification is recorded below; the known Hero mask/edge artifact remains deferred.

Source reference: `hero_carousel_design_spec_en.md` v2.1, supplied from the user's external prototype workspace. Its Web implementation, credentials, mock data, and claimed acceptance results are not native requirements or implementation evidence. Credentials must not be copied into project documentation.

This specification follows [CONTEXT.md](../CONTEXT.md), the [jellypilot-ui design system](design-system.md), [ADR 0027](adr/0027-cross-platform-iced-frontend.md), and [ADR 0028](adr/0028-library-image-raster-pipeline.md). The existing [Sidebar specification](sidebar-design-spec.md) continues to own shell geometry and account interaction.

## Accepted Paper Home Synchronization

Reference: [Desktop - Home, MY-0](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/MY-0). The decisions below are confirmed requirements for this implementation, not a claim of completed implementation or visual acceptance.

Confirmed decisions:

- Synchronize the entire Home composition, including the Sidebar, Account Popover, and bottom playback bar, not only the central Video Home content. Shared shell changes can affect other destinations.
- Add Favorites and Watchlist actions beside the Hero playback and Details actions. For a movie, both actions target that movie. For a featured episode, both target its parent series, while playback continues to target the episode. This supersedes the previous prohibition against adding those actions from the prototype.
- Prioritize the Hero composition at smaller/shorter window sizes. Continue Watching may move below the first screen; the previous requirement that its complete cards and captions fit on the first screen no longer governs this revision.
- The previously reported Hero edge/corner artifact remains recorded and deferred; synchronizing the design does not reopen that renderer investigation.
- Keep the original Backdrop uncropped and use a fixed 440-logical-pixel foreground Hero rather than deriving Hero height from the image's aspect ratio. Preserve the full-width, original-aspect background, with excess image height extending behind later content instead of being clipped at the Hero boundary. Logo, actions, and the selection rail are positioned against the fixed foreground region.
- Keep Title Logo priority with the existing text-headline fallback, rather than switching every item to the reference's plain-text title.
- Keep other real Home sections, including latest Episodes and library shortcuts where available. The three illustrated sections are examples, not a new restriction on Home content.
- Adapt dark and light themes separately using the existing semantic palette and the same layout. The dark reference does not make the Home composition or its shared shell permanently dark; the Backdrop gradient blends into the active theme.
- Do not automatically adjust the page's vertical scroll offset when switching the Featured Item. A different image aspect ratio must not change the fixed foreground Hero height.
- The bottom playback bar's Favorites action targets the currently playing movie or the parent series of the currently playing episode. It is independent of the Hero selection.
- Keep a directly accessible Stop button in the restyled playback bar, even though the reference omits it from the primary transport group.
- The playback bar's expand action toggles the current player's fullscreen state. It does not make the Library Browser fullscreen, open a new player page, or introduce a playback backend.
- Icons may be obtained directly from [Reicon](https://reicon.dev). Reuse the existing vendored set where suitable and obtain missing glyphs from Reicon rather than drawing approximations; retain the semantic icon helpers, theme tinting, and asset attribution required by [ADR 0034](adr/0034-reicon-icon-set.md).

Acceptance criteria for this revision (human visual acceptance remains outstanding):

- Compare the complete composition with the linked Home artboard at its 1440px reference width and at the supported 1760×900 and 1024×640 window sizes, in both themes. Account Popover appearance is checked when opened; it is not permanently open merely because the reference illustrates it.
- Verify uncropped Backdrops with landscape, portrait, and differing candidate aspect ratios while the foreground Hero height stays fixed. Continue Watching may require scrolling. Changing selection must not explicitly adjust the page's vertical scroll offset; normal scroll-range clamping still applies when content becomes shorter.
- Verify Logo priority, readable episode identity and controls, distinct selected/focused rail states, and retained real Home sections. Missing or failed artwork retains usable metadata/actions and an honest neutral state rather than blocking playback or displaying fabricated media.
- Feature episode A from series X while episode B from series Y is playing: Hero Favorites/Watchlist act on X, the playback-bar Favorite acts on Y, and playback actions still target the appropriate episode. Missing parent identity or unresolved collection state must not silently turn a series action into a single-episode action or show a fabricated success.
- Verify successful and failed collection changes against the existing server-confirmed Favorites and local, Profile Scope-owned Watchlist contracts, including selected-item and profile changes during an operation.
- Verify the restyled bar retains direct Stop, queue, audio/subtitle selection, volume and seeking where available, and that fullscreen affects the current player rather than the browsing shell. No Now Playing state means no fabricated active bar or progress.
- Verify new and reused Reicon glyphs remain theme-tinted with clear active, disabled, hover, and keyboard-focus states. Hero decorative borders stay absent; the previously recorded mask/edge artifact remains a known deferred issue, not a failed promise of renderer repair.

The specification below describes the preceding implementation and remains the baseline except where these confirmed decisions supersede it. Other behavioral or visual differences are not silently adopted from the reference.

## 1. Purpose and Scope

Video Home prioritizes recovering interrupted viewing, including movies, over showcasing the next episode of a series. The Hero is a prominent presentation of a selected item, not the only route to resume other items.

The current visual trial preserves the accepted selection/resume interactions and uses this composition:

- Artwork becomes the full-bleed background of Video Home's top region, not a rounded or inset Hero card.
- One original Backdrop spans the full content width, preserving its source aspect ratio and extending naturally downward behind the foreground and home rows. There is no ambient-color extension, edge feathering, stretching, or repetition.
- Title treatment, essential metadata, and playback/Details actions form one lower-left foreground group. A detail-style vertical gradient transitions from transparent at the top through a near-opaque metadata/action zone to the page color at the image bottom; there is no separate left-side scrim.
- An image-only Hero Selection Rail sits quietly at the lower right. Full title, episode identity, and resume/next-up/latest role appear on hover or keyboard focus rather than as a second permanent metadata row.
- The background scrolls away with Home content. It is not fixed wallpaper behind later rows.
- Keep the foreground Hero at 440 logical pixels; Continue Watching remains directly resumable but may require vertical scrolling.

The Paper synchronization includes Home, shared Sidebar/Account Popover composition, and the bottom playback bar. It preserves account lifecycle, Settings, playback backends, and separate Next Up/latest-content browsing surfaces.

## 2. Two Distinct Interactions

| Surface | Meaning | Activation |
| --- | --- | --- |
| Hero | Presents the Featured Item | Its primary action plays or resumes that item; Details opens that item's detail view |
| Hero Selection Rail | Chooses what the Hero presents | Activating a card changes the Featured Item, without starting playback |
| Continue Watching | Direct access to unfinished viewing | Activating a card directly resumes that card's item, without first selecting it in the Hero or visiting Details |

The same item may appear in both the Hero Selection Rail and Continue Watching. This duplication is deliberate: eliminating it must not introduce a two-step resume interaction. The selection rail is not a playback queue and does not imply automatic next-item playback.

The design uses manual selection. Automatic rotation, countdown bars, a carousel play/pause control, and global carousel shortcuts are not adopted from the prototype.

## 3. Candidate Selection

1. Use the current Profile Scope's Continue Watching items first, followed by Next Up.
2. Preserve the server's ordering within each source; do not introduce a local recommendation score.
3. Include the same media item only once in the selection rail, with Continue Watching taking precedence over a duplicate from Next Up.
4. Represent each series at most once in the Hero candidates. A resumable item wins over that series' Next Up item; if multiple items qualify within the same source, use its first item. Movies remain independent candidates.
5. Series grouping affects only Hero candidates. It never removes other episodes from the direct-resume Continue Watching row or the existing Next Up row.
6. Only when both continuation sources have successfully resolved empty, use existing latest-content rows as the Hero source, preserving their existing order. Label this state as latest content rather than Continue Watching or Next Up; its normal action is Play, not a fabricated resume action.
7. If no real candidates exist, omit the Hero and its selection rail. Retain the rest of Video Home's real content and appropriate empty states.

Next Up is the server's episode-continuation result, not a guarantee of zero playback progress. The current Jellyfin query enables resumable episodes, so source membership alone must not determine whether an action resumes or starts from the beginning.

Candidate identity is scoped to the provider, server, and user. Group series by their actual identity, never by display title; absent series identity must not merge unrelated episodes. Use the existing bounded home responses rather than fetching the whole library to populate the rail. There is no arbitrary ten-item design requirement.

## 4. Selection Stability

Keep a manually selected Featured Item while it remains an eligible candidate in the same Profile Scope and application run. Returning to Video Home or refreshing its data does not reset a valid selection to the first candidate.

Example: the user selects B in the Hero, then resumes A from Continue Watching. On returning, B remains featured if still eligible. Direct playback of A is not an implicit Hero-selection action.

Track eligibility by item identity, not the item's previous index. If the selected item disappears from the computed candidates, choose the first eligible item using the ordering above. A restart or Profile Switch initializes the selection again; this design adds no persisted selection preference. Entering Control-Only continues to discard Library Browser state under [ADR 0030](adr/0030-resource-bounded-control-only-composition.md), so a later return to Full mode also initializes a new selection.

## 5. Images and Information

### Hero

- Use a movie's Backdrop or an episode's series Backdrop as the principal image. As in Details, fall back to the item's Primary image when no Backdrop is available; episodes keep the order series Backdrop → item Backdrop → item Primary. This is image-reference selection, not a retry policy for failed downloads.
- Use its Title Logo, or the series Title Logo for an episode, with the existing text-headline fallback. Keep logo aspect ratio and alpha; do not introduce a portrait poster slot.
- Show the episode number and episode title when the Featured Item is an episode. A shared series Backdrop must not make the concrete playback target ambiguous.
- Keep the foreground action-led: title treatment, essential episode identification, available duration, and the primary playback action plus Details. Observable progress remains on the separate Continue Watching cards, not as a detached Hero indicator.
- Leave the synopsis in Details. Do not add an always-visible or wide-window synopsis.
- Do not add Watchlist or other actions merely because they appear in the prototype wireframe.

### Image cards

Episode cards in the Hero Selection Rail and Continue Watching use Episode Stills with the existing fallback order: episode Primary, episode Thumb, series Thumb, series Backdrop. The rail uses image-only selection targets with full-value hover/focus hints; Continue Watching keeps its identifying labels and progress. Movie cards retain the existing landscape-card image policy.

Continue Watching retains observable progress. Do not fabricate percentages or remaining minutes for missing or invalid duration data.

### Composition priority

1. Keep the identity, playback action, and important text legible.
2. Preserve the visible subject of the principal image rather than blindly forcing every image into one crop.
3. Size the Backdrop from the available content width, independently of the foreground Hero height and row positions.

The bottom gradient may obscure lower image content, but the image geometry keeps its source ratio and full width. This trial does not commission subject detection, face detection, or generated artwork.


## 6. Height and Responsive Priorities

Keep the existing Full-mode default of 1760×900 logical pixels and minimum of 1024×640; do not change window constraints to make this design fit.

The foreground Hero is fixed at 440 logical pixels at both supported sizes. Its height is independent of the viewport height and Backdrop ratio. Continue Watching may fall below the first screen; vertical scrolling must expose complete cards, captions, and direct-resume controls.

At reduced width, keep the selection rail subordinate and compact, truncate long labels with full-value hints, and preserve separate selection and direct-play interactions. The width-sized background remains independent and can extend behind rows without pushing them down. Selection does not explicitly change the page's vertical offset.

Both supported themes and the existing reduced-motion preference remain in scope. Do not force the entire homepage dark or replace global theme tokens to imitate the prototype's white CTA and cinema palette. The Backdrop gradient must blend into the active native theme through the existing design system, not a second styling mechanism.

## 7. Confirmed Interaction and Failure Boundaries

These interaction and failure rules were included in the final shared-understanding confirmation:

- Hover alone does not change the Featured Item or launch playback. Explicit selection changes the Hero.
- Keyboard input is scoped to the focused surface. Selecting a rail card never launches MPV; activating a direct-resume card does. Do not intercept Enter, Space, or arrow keys globally while the user operates search, menus, or another control.
- Distinguish the selected rail card from keyboard focus. Manual selection keeps its card visible without moving focus to the Hero or scrolling the whole page.
- Rail navigation is bounded rather than an infinite loop. When only one candidate exists, omit redundant selection/navigation controls and show a static Hero.
- Selection updates the title, metadata, primary action, and Details target together. Late image completion must never restore an old candidate's information or controls.
- Do not wait for decorative imagery before allowing playback of valid metadata. Missing artwork settles into a neutral, theme-consistent image state with text identification; it must not shimmer forever or substitute an unrelated item.
- Empty, loading, and failed content are distinct. Pending or failed continuation requests are not proof that both sources are empty and do not trigger the latest-content fallback. Retained same-profile data, if available, remains governed by the existing loading/error policy.
- Preserve real server errors, playback availability, server progress reporting, and External MPV Playback. Do not adopt the prototype's fake-media fallback or GET-only playback simulation.
- Any short transition must honor reduced motion and remain interruptible. No sustained timer is needed for an idle manually controlled Hero. Artwork work stays behind the existing bounded raster pipeline; do not preload full-resolution layers for every slide merely to imitate the Web implementation.

## 8. Human Acceptance Checklist

This is the outstanding human acceptance checklist, not a record of completed visual verification:

- At 1760×900 and 1024×640 logical pixels, with and without the playback bar—including active intro/credits prompts—verify the fixed Hero remains usable and scrolling exposes complete Continue Watching cards and direct-resume targets.
- Verify the original image reaches the Home region's top and side edges, retains its aspect ratio, and extends below the foreground without a rounded frame, blurred filler, or feathered edges. Scroll down: the image must move with the page rather than remain fixed behind later content.
- Check that title, metadata, and actions read as one foreground group rather than separate blocks. Bright artwork must remain readable without turning the entire left half into a solid panel.
- Verify the quiet thumbnail rail retains a clear selected item and a separate keyboard-focus indicator. Hover/focus must expose the full title, episode, and action/source role without changing selection.
- Select B in the Hero and resume A from the separate row in one activation. Verify A resumes at its own position, and B remains selected on return when still eligible.
- Verify one series with multiple unfinished episodes occupies one Hero candidate without losing those episodes from Continue Watching.
- Verify an item returned in both Continue Watching and Next Up appears once in the selection rail and has the appropriate resume action.
- Verify empty continuation sources use real latest content with honest labeling; pending, failed, and entirely empty sources do not look like a populated mock library.
- Inspect bright, dark, low-resolution, and missing Backdrops; left-edge and multi-subject compositions; wide and narrow logos; long titles; and episode identification. Check readable text and sensible framing rather than assuming a fade guarantees contrast or subject preservation.
- Verify pointer and keyboard selection, direct playback, focus versus selection, search input, menus, reduced motion, and both themes.

Verification follows the [validation policy](agents/validation.md). Focused core/MPV/iced tests, focused iced clippy, `bun run check`, workspace Rust tests, and the native startup smoke gate passed. The final Detail refresh-ordering fix was rechecked with iced tests, iced clippy, and Rust format checking. Regressions cover independent resume after scrolling, uncropped Backdrop geometry, menu keyboard-focus retention, collection confirmation after navigation, conflicting same-item write rejection, fullscreen replacement admission, and browser scroll preservation across embedded fullscreen. Independent source review findings were addressed. These are code-level/startup results, not visual acceptance or live-server playback verification.
