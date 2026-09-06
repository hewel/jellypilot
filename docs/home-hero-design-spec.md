# JellyPilot Video Home Hero and Resume Interaction Specification

**Status: Full-width Backdrop trial implemented; awaiting human evaluation.** After reviewing the previous composition, the user requested this trial without ambient fill or edge feathering. Candidate, interaction, layout-boundary, workspace, and native startup checks have passed. The checklist below remains unchecked: this is not human visual acceptance or a live-server playback result.

Source reference: `hero_carousel_design_spec_en.md` v2.1, supplied from the user's external prototype workspace. Its Web implementation, credentials, mock data, and claimed acceptance results are not native requirements or implementation evidence. Credentials must not be copied into project documentation.

This specification follows [CONTEXT.md](../CONTEXT.md), the [jellypilot-ui design system](design-system.md), [ADR 0027](adr/0027-cross-platform-iced-frontend.md), and [ADR 0028](adr/0028-library-image-raster-pipeline.md). The existing [Sidebar specification](sidebar-design-spec.md) continues to own shell geometry and account interaction.

## 1. Purpose and Scope

Video Home prioritizes recovering interrupted viewing, including movies, over showcasing the next episode of a series. The Hero is a prominent presentation of a selected item, not the only route to resume other items.

The current visual trial preserves the accepted selection/resume interactions and uses this composition:

- Artwork becomes the full-bleed background of Video Home's top region, not a rounded or inset Hero card.
- One original Backdrop spans the full content width, preserving its source aspect ratio and extending naturally downward behind the foreground and home rows. There is no ambient-color extension, edge feathering, stretching, or repetition.
- Title treatment, essential metadata, and playback/Details actions form one lower-left foreground group. A detail-style vertical gradient transitions from transparent at the top through a near-opaque metadata/action zone to the page color at the image bottom; there is no separate left-side scrim.
- An image-only Hero Selection Rail sits quietly at the lower right. Full title, episode identity, and resume/next-up/latest role appear on hover or keyboard focus rather than as a second permanent metadata row.
- The background scrolls away with Home content. It is not fixed wallpaper behind later rows.
- Subject visibility, readable controls, and a complete first-screen Continue Watching row take precedence over a fixed Hero height.

The change is scoped to Video Home's Hero, its selection rail, and the layout needed to preserve direct resume access. It does not redesign the Sidebar, account lifecycle, Settings, detail screens, or playback architecture. Existing Next Up and latest-content rows remain separate browsing surfaces.

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

- Use a movie's Backdrop or an episode's series Backdrop as the principal image.
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

The first-screen priority is the usable Continue Watching row, not a fixed minimum Hero height. When that row exists, its visible cards, titles, and direct-resume targets should fit without vertical scrolling at the supported default and minimum window sizes, accounting for persistent shell and playback controls. Horizontal browsing remains necessary for additional items; the requirement does not put every candidate on screen simultaneously.

At reduced height:

- Reduce the foreground Hero and Logo presentation height. The width-sized background remains independent and can extend behind the rows without pushing them down.
- Make the Hero Selection Rail visibly subordinate and more compact than the direct-resume row; do not duplicate two full-sized episode-card rows.
- Reduce spacing and secondary metadata before reducing the readability or usability of titles and buttons.
- Preserve the separate selection and direct-play interactions.
- Do not reserve the prototype's 440–520px Hero minimum or force a 2.2:1/2.4:1 ratio against the available height.

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

- At 1760×900 and 1024×640 logical pixels, with and without the playback bar—including active intro/credits prompts—verify the visible Continue Watching cards, identifying text, and direct-resume targets remain on the first screen.
- Verify the original image reaches the Home region's top and side edges, retains its aspect ratio, and extends below the foreground without a rounded frame, blurred filler, or feathered edges. Scroll down: the image must move with the page rather than remain fixed behind later content.
- Check that title, metadata, and actions read as one foreground group rather than separate blocks. Bright artwork must remain readable without turning the entire left half into a solid panel.
- Verify the quiet thumbnail rail retains a clear selected item and a separate keyboard-focus indicator. Hover/focus must expose the full title, episode, and action/source role without changing selection.
- Select B in the Hero and resume A from the separate row in one activation. Verify A resumes at its own position, and B remains selected on return when still eligible.
- Verify one series with multiple unfinished episodes occupies one Hero candidate without losing those episodes from Continue Watching.
- Verify an item returned in both Continue Watching and Next Up appears once in the selection rail and has the appropriate resume action.
- Verify empty continuation sources use real latest content with honest labeling; pending, failed, and entirely empty sources do not look like a populated mock library.
- Inspect bright, dark, low-resolution, and missing Backdrops; left-edge and multi-subject compositions; wide and narrow logos; long titles; and episode identification. Check readable text and sensible framing rather than assuming a fade guarantees contrast or subject preservation.
- Verify pointer and keyboard selection, direct playback, focus versus selection, search input, menus, reduced motion, and both themes.

Verification follows the [validation policy](agents/validation.md). Code-level checks cover candidate grouping, cross-source selection retention, long-label rail scrolling, late-artwork focus retention, and complete Continue Watching caption bounds with docked playback controls. The headless native scenario draws the composed page and checks actual two-dimensional text/control separation at both supported window sizes, with and without playback controls and active prompts. It also checks full-width landscape/portrait Backdrop geometry and unchanged Continue Watching positions after image arrival. Workspace checks, workspace tests, and the native startup smoke gate pass; independent source reviews reported no outstanding findings. Human visual acceptance and live-server External MPV Playback remain separate.
