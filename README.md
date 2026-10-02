# GPSART design prototype

Responsive HTML screens for the GPS art route-planning app.

## Run locally

```sh
npm ci
npm run dev
```

Open the URL printed by Vite. `npm run build` produces the HTML pages in `dist/client`. `npm run test:sites` checks the included hosting worker.

## Design pages

- `index.html`: planner and shape/text catalogue; generation, results, route details and recovery states.
- `login.html`: Google sign-in, pending, cancelled/error, welcome and account states.
- `routes.html`: route comparison.
- `saved.html`: routes stored in this browser.
- `screens.html`: screen directory for design review, including failure states.

Shared UI code is in `src/app.js` and `src/styles.css`; image assets are in `public/assets`.

## Developer integration

This is a frontend design prototype. The map, geometry, distances, route generation and Google authentication are simulated and labelled in the UI. Sample GPX files are for testing, not navigation. Connect the real routing, geocoding and Google authentication services before production use. Signing in does not enable cloud sync; saves use browser local storage.

The product epic is in `docs/problem-statement.md`. The implementation architecture and prioritized engineering backlog are in `docs/technical-stories.md`. Google login screens are an additional design requirement; guest planning remains available.

Validation: production build and hosting-worker checks pass. Visual QA against the selected concept has not been completed.
