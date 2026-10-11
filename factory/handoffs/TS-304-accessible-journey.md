# TS-304 responsive and accessible journey handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-305 and later browser journey work  
**Compatibility:** changes to focus restoration, planner field IDs/error relationships, design-tab keyboard behavior, progress announcements, dialog behavior, or the 360 px layout require this handoff and the TS-304 checks to be updated together

The browser shell preserves focus across same-screen rerenders by stable control identity and scrolls to the top only when the routed screen changes. The design-category control follows the tab pattern: one selected tab is in the tab order, Arrow Left/Right and Home/End change the selected category, and focus follows selection.

Planner validation uses `start-error` and `distance-error`, referenced from the corresponding fields. Invalid state is expressed through text, `aria-invalid`, and a stronger border; selected state uses native checked state or ARIA selection/pressed state. Exact text start entry and immutable route instructions remain the non-map path through planning and route use.

The saved-route rename action uses the native `rename-route-dialog`, focuses `rename-route-name`, and returns focus to the rename button after close. Mobile panels are non-modal document content and do not trap focus. Route generation uses a named step list with `aria-current="step"`; the persistent `#announcer` emits a polite update only for a newly observed job/stage key.

The responsive contract covers 360, 768, and 1440 px without page-level horizontal overflow. The production audit also exercises catalogue, saved, results, and login surfaces at 360 px. Muted text, the login separator, and Leaflet attribution links meet the automated contrast/link-distinction checks; mobile auth content can shrink below its desktop intrinsic width; every north indicator has image semantics; and reduced-motion disables animation.

Verify with:

```text
npm exec -- tsx --test tests/accessibility.test.ts
npm run test:a11y
npm run test:unit
npm run test:visual-shell
npm run factory:verify
```
