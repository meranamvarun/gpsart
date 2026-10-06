# TS-304 responsive and accessibility verification

Verified on 2026-10-06 against the production client build.

## Acceptance checklist

1. The automated browser audit loaded the planner at 360 × 800, 768 × 900, and 1440 × 900. At every width, `documentElement.scrollWidth <= clientWidth` and the primary action could be scrolled fully into the viewport.
2. Inputs, activity choices, category tabs, design choices, progress steps, result controls, status/error messages, map alternatives, and the saved-route rename dialog use native elements or explicit accessible names, relationships, state, and roles. All keyboard-focusable controls retain the shared visible 3 px focus indicator.
3. Invalid planner fields receive `aria-invalid`, field-specific `role="alert"` messages, and matching `aria-describedby` references. Error text begins with “Error:” visually, while selected controls expose `aria-pressed`, `aria-selected`, or native checked state.
4. The native modal rename dialog focuses its labelled input on open and restores focus to its opener on close. The mobile panel remains a normal document region rather than a focus trap. Job stages are exposed as a named step list; a persistent polite announcer changes only when the observed job stage changes.
5. Exact latitude/longitude entry, on-demand geolocation, and search selection remain available without clicking the map. Candidate details retain the complete traversal list and structured text instructions when map tiles fail.
6. `axe-core` WCAG 2 A/AA, 2.1 AA, and 2.2 AA checks reported zero serious or critical findings at all three widths. The keyboard smoke confirmed the skip link is first, planner controls are reachable, arrow keys move both selection and focus between tabs, and focus remains visibly outlined. Chromium’s platform accessibility tree exposed “Starting point” as a textbox, “Shapes”/“Text” as tabs, and activity choices as buttons, providing the screen-reader smoke for names and roles.

## Evidence

- `docs/evidence/ts-304-mobile-360.png`
- `docs/evidence/ts-304-tablet-768.png`
- `docs/evidence/ts-304-desktop-1440.png`
- `docs/evidence/ts-304-axe-report.json` (includes viewport metrics, findings, keyboard order, tab behavior, and the accessibility-tree sample)

The screenshots were visually inspected for clipped content, horizontal overflow, unreachable actions, and unintended visual-language changes. The 360 px view stacks the map and planner, while the 768 px and desktop views keep the planner and map side by side.

## Commands

```text
npm exec -- tsx --test tests/accessibility.test.ts
npm run test:a11y
npm run test:unit
npm run test:visual-shell
npm run factory:verify
```
