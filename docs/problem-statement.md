# Web App Epic for Routes That Draw Shapes and Text

Version 1.0\
Prepared 2 October 2026\
Epic ID MAPART 001

Build a responsive web application that turns a starting point, a selected shape or text design, an activity type, and a target distance into a practical route on a map. When the user follows that route, their recorded activity should resemble the selected design.

This specification defines the product experience, a first release, development stories, acceptance criteria, routing rules, API boundaries, and release gates. The starting point, catalogue selection, and distance are the core requirements supplied by the product owner. Scope limits, numerical defaults, architecture, and success targets below are proposed implementation decisions, not existing capabilities or measured results.

## 1 Epic statement

**As a runner, cyclist, or walker, I want to choose where I start, select a shape or text design, and enter how far I want to travel, so that I can follow a route that creates that design on the map.**

The main product outcome is a usable route with a recognisable visual result. The map preview, distance, route instructions, and exported track must all describe the same ordered journey.

An illustrative request is: start at a selected public entrance in Koramangala, choose cycling, select a heart, and request 15 km. The application searches the surrounding routable network and presents up to three qualifying routes. A candidate might be 14.6 km and include a short connecting section. These numbers illustrate the intended experience; they are not evidence that this route exists at that location.

## 2 Users and the problem being solved

The primary user is a recreational runner, cyclist, or walker who wants to make an activity more creative without manually constructing a large sequence of map waypoints. A secondary user is someone planning a themed outing, such as a birthday ride or a club activity, using a recognisable symbol or short message.

The application must handle the compromise between the requested design and the available streets and paths. The selected start, target distance, and design will not always fit together. Returning an honest unsuccessful search is a valid outcome. Displaying a beautiful outline that cannot be followed as one continuous journey is a product failure.

## 3 First release and later scope

### First release

The first release includes a working mobile and desktop planner, manual and current-location start selection, walking/running/cycling profiles, a curated catalogue containing both shapes and text, target distance controls, asynchronous route generation, candidate comparison, route details, GPX export, and saving routes on the current device.

Browsing and planning do not require an account. A bounded pilot area is configurable and visible in the UI. Bengaluru is a proposed first pilot because it matches the initial use case; the routing core must not hardcode one city. A location outside supported coverage receives a clear message.

Suggested seed catalogue: heart, circle, triangle, square, star, and the text designs HI, GO, and RUN. These are candidates for evaluation. Publish each template only after its stroke plan and recognisability have passed review. More complex designs may have fewer matches. Both shapes and text must be represented in the released catalogue.

### Later releases

Later stories cover short custom text, foreground route following, account-based cross-device saving, explicitly shared routes, and comparison with an imported recorded activity. Freehand drawing, arbitrary image conversion, a community feed, competitions, subscriptions, and reliable background activity recording are outside the first release.

Selecting a curated text design is part of the first release. Typing an arbitrary word is an extension because character combinations introduce additional stroke connections and feasibility problems.

## 4 Proposed product defaults

All numerical values in this section are configurable starting points to validate during the routing prototype.

| Control             | Proposed default               | Required behaviour                                                                           |
| ------------------- | ------------------------------ | -------------------------------------------------------------------------------------------- |
| Starting point      | User selects a pin or location | Never silently move the requested start to another neighbourhood                             |
| Activity            | Require an explicit selection  | Walking, running, or cycling changes the routing profile                                     |
| Design              | Require an explicit selection  | Choose from Shapes or Text; show a thumbnail                                                 |
| Target distance     | 10 km                          | Accept 1 to 100 km in 0.1 km increments within supported pilot limits                        |
| Distance tolerance  | Plus or minus 10 percent       | Advanced choices of 5, 10, or 20 percent                                                     |
| Return to start     | On                             | Off allows the route to end elsewhere and shows the finish before selection                  |
| Candidate count     | Up to 3                        | Show fewer when only fewer qualify; never duplicate results to fill slots                    |
| Text orientation    | Upright on a north-up map      | Permit at most 15 degrees of rotation either way; no mirroring                               |
| Shape orientation   | Rotation allowed               | Preserve the template proportions; mirroring only for templates explicitly marked equivalent |
| Generation deadline | 60 seconds                     | Return qualifying results already found or a clear bounded-search outcome                    |

Distance is the total distance the user must travel, including connectors, repeated sections, and the return leg when enabled. The product must not display only the artistic portion as the workout distance.

## 5 End to end user journey

1. The user opens the planner and sees the supported coverage, an interactive map, and a compact planning panel.
2. They search for a place, click the map, enter coordinates, or choose Use my location. A start marker appears and can be moved.
3. They choose walking, running, or cycling.
4. They browse the Shapes or Text catalogue and select a design. Selection alone does not claim the design is feasible at this location.
5. They enter a target distance and optionally change tolerance or the return-to-start setting.
6. They select Generate routes. The application keeps their inputs visible while showing the current generation stage.
7. The application returns up to three candidates or explains that no qualifying match was found in the search budget.
8. The user compares the complete route preview, actual distance, visual resemblance, endpoint, repeated sections, and available path information.
9. They open one candidate, inspect its direction and instructions, and save it on the current device or export its GPX track.
10. If the result is unsuitable, they edit the request and generate again. The previous result remains available as a previous result, clearly associated with its original inputs.

## 6 Screens and interaction requirements

| Screen or state      | Required content                                                                             | Main actions                                 |
| -------------------- | -------------------------------------------------------------------------------------------- | -------------------------------------------- |
| Planner              | Map, start marker, activity, design selector, target distance, return setting, coverage      | Set start, select design, generate           |
| Design catalogue     | Shapes and Text tabs, thumbnails, readable names, complexity hints                           | Select design, return to planner             |
| Generating           | Request summary, truthful stage, elapsed time, cancellation                                  | Cancel, retain inputs                        |
| Results              | Up to three complete previews and comparable route metrics                                   | Select candidate, refine request             |
| Route details        | Ordered route, start/finish, distance breakdown, instructions, available surface information | Export GPX, save locally                     |
| No match             | What was searched and changes that could help                                                | Adjust distance, tolerance, design, or start |
| Saved on this device | Named route snapshots and their request summaries                                            | Reopen, rename, delete, export               |

Desktop may use a side panel beside the map. Mobile should use a collapsible bottom panel with a persistent, reachable primary action. Controls must not obscure the selected route or start marker. Map interaction must have a text-based alternative for setting the start and inspecting route details.

The design preview may show an optional ideal-outline overlay. The default results view must show the complete travelled trace, including the visually inconvenient sections. An ideal outline must never be presented as a generated route.

## 7 Routing rules

### Constraints every returned route must satisfy

- The route is one continuous ordered traversal through the supported routing network. Visual crossings do not establish connectivity between a bridge and the road below it.
- Every traversed edge and turn is permitted by the selected activity profile and the available map restrictions. The engine must account for direction, turn restrictions, and access metadata, including profile-specific exceptions where available.
- A line across a lake, building, railway, disconnected path, or unmapped shortcut is not an acceptable substitute for a routed connection.
- The requested start remains fixed. If it is off the routable network, the app identifies the proposed access point and the distance from the pin. A snap of up to 30 metres may be shown for confirmation; a larger relocation requires the user to choose a new start. The distance between the pin and network is not invented as a traversable segment.
- When return to start is on, the ordered route ends at the same confirmed routable start. Coordinate serialization may differ by at most 5 metres; this tolerance does not authorize an unconnected final leg.
- Exact distance is not guaranteed. A normal result must lie within the selected distance interval. Out-of-range routes, if retained internally, must not be passed off as satisfying the request.
- The current hard exclusions in the pilot are motorways and roads marked motorroad, access forbidden for the activity, ferries, and steps for cycling. Missing metadata is treated according to a documented conservative profile policy, not labelled as verified access.
- Current closures and real-world conditions may be absent from the map. The product displays data provenance when available and does not describe generated routes as guaranteed safe or open.

### Rules for text and multiple strokes

Text often consists of separate strokes or letters. The user still travels one continuous route. The template therefore needs an explicit stroke order, connecting travel between strokes, and any required retracing.

Every connector must appear in the full preview, contribute to total distance, and participate in the visual-quality assessment. Pausing a recorder or removing travel from the exported track is not an acceptable way to make a text design look cleaner. Text must not be mirrored, have its letters reordered, or be rotated until it is unreadable on a north-up map.

### Distance and quality accounting

Compute total distance from the ordered traversals of the canonical routed geometry. A road used twice contributes its length twice. Distance along unique road segments is a separate diagnostic, never the workout total.

Partition traversal occurrences into artwork travel and connecting travel. Their distances sum to the total. Repeated distance is an overlapping diagnostic and must not be added again to that sum.

Assess resemblance on the complete displayed trace. Use separate penalties for missing intended strokes, unwanted connecting lines, excessive retracing, distorted proportions, and an indistinct text result. A candidate cannot compensate for an illegal edge by receiving a high shape score.

Use understandable quality bands such as Strong match and Approximate match. Numeric algorithm scores are internal until calibrated; do not present an unvalidated score as a percentage probability of a successful activity.

## 8 Development stories and acceptance criteria

P0 means required for the first release. P1 means an extension after the first release passes its routing and usability gates.

### US01 Select and confirm a starting point

**Priority P0**

As a user, I want to select my starting point on a map so that the suggested activity begins where I intend to start.

Acceptance criteria:

1. Given a supported location, when I search for it, place a pin, or enter valid coordinates, then the map shows a labelled start marker and an editable location summary.
2. Use my location requests browser permission only after I select it. Denial, timeout, unsupported geolocation, or a poor position fix leaves manual selection available.
3. Moving the marker updates the request and marks existing candidates as belonging to the previous request.
4. An off-network start follows the explicit confirmation rules in Section 7. Both requested and confirmed points are retained in the request record.
5. Unsupported coverage produces a specific message before an expensive generation job is accepted. A place-name match alone is not proof of routing coverage.

### US02 Choose the activity profile

**Priority P0**

As a user, I want routes appropriate for walking, running, or cycling so that the selected route reflects how I will travel.

Acceptance criteria:

1. Given the same start and design, when the activity changes, then generation uses the corresponding versioned routing profile and invalidates previous candidate eligibility.
2. Running and walking may share network access rules in the pilot, while duration estimates use different configurable pace assumptions.
3. Cycling excludes steps and forbidden or unsuitable edges according to the pilot profile. A car route is never silently substituted.
4. The result identifies the activity and any available surface or access qualifications. Missing elevation or surface data is displayed as unavailable rather than zero or paved.

### US03 Select a shape or text design

**Priority P0**

As a user, I want to choose from available shapes and text designs so that I can decide what my activity should draw.

Acceptance criteria:

1. The catalogue includes both Shapes and Text with accessible thumbnails, names, and a visible selected state.
2. Each published item references a versioned ordered template, orientation rules, valid proportions, and a reviewed plan for joining strokes.
3. Changing the design preserves the start, activity, and distance but marks old results as previous results.
4. Catalogue presence means the template is supported, not that a matching route exists everywhere. Local feasibility is established only by generation or a completed feasibility check.
5. A disabled or unavailable template explains why it cannot be selected without clearing the rest of the form.

### US04 Configure distance and the finish

**Priority P0**

As a user, I want to set my travel distance and whether I return to the start so that the route fits my activity plan.

Acceptance criteria:

1. The form rejects empty, nonnumeric, nonfinite, zero, negative, and unsupported distance values on both client and server.
2. The form shows the accepted range and the resulting distance interval. A 10 km request at 10 percent tolerance allows 9 to 11 km, inclusive.
3. All comparisons use canonical metres before display rounding; rounding cannot make an out-of-range candidate eligible.
4. Return to start defaults on. When off, every candidate clearly identifies a separate finish and states that travel home is not included.
5. Changing a tolerance or endpoint option changes the request revision used by generation and caching.

### US05 Generate routes asynchronously

**Priority P0**

As a user, I want visible generation progress and the ability to cancel so that I can manage a search that takes time.

Acceptance criteria:

1. Given valid inputs, selecting Generate creates a job and displays a request summary without blocking map controls.
2. Double clicks and transport retries with the same idempotency key do not create duplicate work. Reusing a key with different inputs returns a conflict.
3. Show truthful stages such as Checking roads, Fitting design, and Comparing routes. Do not invent a percentage complete when the search has no meaningful denominator.
4. Cancellation records a terminal cancelled state and prevents late results from replacing the active request. In-flight provider calls may finish, but no further search is scheduled.
5. At the time budget, publish any fully validated qualifying results already found. Otherwise distinguish no match within the search budget, timeout, and dependency failure.
6. Refreshing the page can resume polling the current unexpired job through the guest session. Results for an older request revision cannot overwrite newer inputs.

### US06 Fit the design to a real route

**Priority P0**

As a user, I want the generated route to resemble my design while following a continuous network path so that I can actually reproduce the result.

Acceptance criteria:

1. Every returned candidate passes all route validity rules in Section 7 before ranking or display.
2. Fitting considers multiple legal placements, scales, allowed orientations, and entry points while preserving the confirmed start and request settings.
3. Full geometry, leg ordering, repeated traversals, artwork connectors, and return travel are retained in one canonical route record.
4. Each candidate includes its actual total distance, distance difference, endpoint policy, resemblance band, repeated distance, and any quality qualifications.
5. The service is permitted to return no result. It never substitutes a straight-line outline, fabricates graph connectivity, or silently widens distance tolerance.
6. Shape and text fixtures both pass the prototype gate before this story is marked complete. A map UI populated only with handcrafted examples does not satisfy it.

### US07 Compare candidate routes

**Priority P0**

As a user, I want to compare a few distinct routes so that I can choose a balance of visual quality and travel distance.

Acceptance criteria:

1. Results show up to three distinct candidates with comparable thumbnails, full-route distance, deviation from target, finish, quality band, and repeated distance.
2. Selecting a card highlights the associated complete geometry and its direction on the map. All cards use a comparable north-up preview convention.
3. Any label such as Best shape match or Closest distance is supported by that candidate's metrics within the displayed set.
4. Near-identical geometry is deduplicated using a versioned similarity rule. Exact traversal duplicates and trivial point-density changes do not create additional candidates.
5. Ideal-outline overlays and connector highlighting are optional inspection aids. Full travelled geometry remains discoverable and is the default preview.

### US08 Handle a search without a qualifying match

**Priority P0**

As a user, I want a useful explanation when the search finds no suitable route so that I know what to change.

Acceptance criteria:

1. A completed unsuccessful search says no qualifying route was found for this request within the search budget. It does not assert that no route could ever exist.
2. Distinct states cover unsupported coverage, no routable access, disconnected network, insufficient visual match, no match within distance, provider unavailability, and exhausted search budget where the evidence supports the distinction.
3. Offer relevant editable suggestions such as a simpler design, greater tolerance, a longer distance, or a different start. Suggestions are not labelled feasible unless checked.
4. Changes occur only after the user chooses them. The original request remains visible and reusable.
5. An out-of-tolerance result can inform a suggested new distance, but cannot appear as a normal qualifying result for the unchanged request.

### US09 Refine a plan without losing context

**Priority P0**

As a user, I want to adjust a request and compare it with the previous attempt so that I can improve the result efficiently.

Acceptance criteria:

1. Start, activity, design, distance, tolerance, and return setting remain editable after a result or unsuccessful search.
2. Editing creates a new request revision. Previously selected routes retain their original input summary.
3. A new generation action cancels or supersedes the previous active job for that planner session.
4. Equivalent requests may use a valid cached computation. A cached candidate still needs exact-start confirmation, current validity checks, and matching profile/template/algorithm versions.
5. The first release does not offer drag-to-edit route geometry. A later geometry editor must revalidate access, distance, continuity, and quality after every edit.

### US10 Inspect the full route

**Priority P0**

As a user, I want to inspect route details before travelling so that I understand the route I am choosing.

Acceptance criteria:

1. Details show start, finish, ordered direction, total distance, approximate duration with its pace assumption, artwork distance, connecting distance, and repeated distance.
2. Instructions and distance markers follow the same canonical traversal as the map. At a self-intersection, the ordering must remain unambiguous.
3. Retraced segments are identified without adding their distance twice to the summary partition.
4. Unknown elevation, surface, or access details remain explicitly unknown. The interface does not manufacture precision.
5. Display the generation time and available map or routing data version. Reopening an old route preserves its snapshot and allows a separate fresh regeneration.

### US11 Export the selected route

**Priority P0**

As a user, I want to export my chosen route so that I can follow it in a compatible route application or device.

Acceptance criteria:

1. Export produces a valid GPX 1.1 file containing the complete ordered track, including connectors and repeated sections. GPX defines tracks as ordered points and supports track segments [R3].
2. The export uses the selected route version, not a newly calculated shortest path between its endpoints.
3. The file is labelled as a planned route. It contains no fabricated recorded-activity timestamps, speed, heart rate, or completion claims.
4. Reimporting the exported coordinates reproduces the stored traversal within the configured serialization tolerance. Export simplification must not alter an intersection, remove a connector, or cut across a prohibited area.
5. The filename is predictable and sanitized. Export errors preserve the selected route and expose a retry action.
6. At least one external GPX importer is checked before release. Compatibility with untested services is not claimed, and no third-party account connection is required for export.

### US12 Save routes on the current device

**Priority P0**

As a user, I want to save and reopen a route without creating an account so that I can plan now and use it later.

Acceptance criteria:

1. Save stores the request summary, route geometry, metrics, instructions, provenance, and route version in browser storage under a user-editable name.
2. The interface says Saved on this device. It does not imply cloud backup or cross-device availability.
3. Reopening displays the saved geometry without silently regenerating it. Expired server job identifiers do not make a complete local snapshot unusable; local export serializes that saved snapshot without requiring an unexpired server result.
4. Rename and delete affect only the chosen local record. Clearing browser storage removes local routes; this limitation is visible where saving is introduced.
5. Storage denial, quota exhaustion, or browser private-mode limits yield a recoverable error and keep GPX export available.

### US13 Support mobile and accessible planning

**Priority P0**

As a user, I want a usable planner on a phone or desktop, including without precise pointer control, so that I can complete the main journey.

Acceptance criteria:

1. The full planning and export flow works at 360 px, 768 px, and desktop widths without horizontal page overflow or an unreachable primary action.
2. Form inputs, catalogue items, candidate cards, and dialogs have names, keyboard access, visible focus, and meaningful error association.
3. A user can enter the start without clicking the map and inspect key route details as text. Colour alone does not distinguish selected routes or error states.
4. Opening and closing mobile panels preserves form state and does not trap focus. Generation state is announced without repeatedly interrupting screen-reader output.
5. Slow map-tile loading does not destroy form input or already generated route data. The UI distinguishes a missing base map from a failed route calculation.

### US14 Bound service work and protect location data

**Priority P0**

As the application operator, I want bounded generation jobs and controlled handling of location data so that the planner remains reliable and respects users' choices.

Acceptance criteria:

1. Server validation caps distance, template complexity, candidate attempts, provider requests, job runtime, response size, and active jobs per guest session.
2. Rate limiting returns a retry interval. Dependency retries use a bounded policy, respect cancellation, and cannot exceed the job's remaining budget.
3. Guest jobs and route snapshots on the server are private to an opaque authorized guest session. Knowing a job or route ID alone does not grant access.
4. The proposed server retention for unsaved guest results is 24 hours. Cleanup is tested; saved local routes follow user-controlled browser storage. Operational logging excludes exact start points, full traces, and future custom text.
5. Provider credentials remain server-side except deliberately public, origin-restricted map credentials. Analytics capture outcomes and timing without raw precise coordinates or full route geometry.
6. Record request correlation, profile/template/algorithm versions, attempts, provider usage, timing, result class, and cancellation. No admin dashboard is required for the pilot.

### US15 Generate a route from short custom text

**Priority P1**

As a user, I want to enter short text so that I can create a more personal route design.

Acceptance criteria:

1. Initially accept one to five uppercase Latin letters or digits from a tested stroke alphabet; display the restriction before submission.
2. Show the complete composed design with connectors before generation. Unsupported characters receive field-level feedback without altering the rest of the request.
3. Character order, readability, orientation, and continuous-travel requirements remain enforced.
4. Excessively complex or poorly fitting text returns an honest no-match result with suggestions such as fewer characters or a greater distance.
5. Custom text is escaped in UI, filenames, logs, and exports and is not inserted into executable content or unrestricted provider queries.

### US16 Follow the route while the web page is active

**Priority P1**

As a user, I want to see my position and route progress so that I can follow the selected route in the browser.

Acceptance criteria:

1. Follow mode requests location only when started and shows permission, accuracy, stale-fix, and unavailable-position states.
2. Progress follows route sequence as well as spatial distance so a crossing or retraced street does not jump the user to the wrong leg.
3. A sustained deviation prompts the user to choose a recovery route. Recovery preserves completed travel and shows the resulting total distance and effect on the final drawing before selection.
4. Rejoining the planned route must not silently replace the remaining design with an ordinary shortest route.
5. Stop ends location watching. Hidden-page or locked-screen gaps are not filled with invented points or reported as continuously tracked travel.
6. The product explicitly scopes this as foreground assistance. The Geolocation specification requires permission and a secure context, and constrains updates to active visible documents [R1]. Reliable background recording requires a separately evaluated platform approach.

### US17 Save across devices and explicitly share a route

**Priority P1**

As a returning user, I want account-based route storage and controlled sharing so that I can reopen a route elsewhere or send it to someone.

Acceptance criteria:

1. Signing in offers an explicit import of selected local routes. It does not silently upload every local record.
2. Cloud routes are private by default and require owner authorization for reads, edits, exports, and deletion.
3. A share action creates a revocable link and previews the geometry and start/finish information that recipients will see.
4. Revocation prevents subsequent access through that link. Any public image option excludes exact coordinates and location labels by default.
5. Trimming a shared illustration for privacy does not mutate the underlying route or claim that a trimmed illustration is a complete navigable plan.

### US18 Compare a recorded activity with the plan

**Priority P1**

As a user, I want to compare my recorded activity with the planned design so that I can see what I actually created.

Acceptance criteria:

1. The user explicitly imports a supported activity file. File size, point count, format, and coordinates are validated before processing.
2. Planned and recorded geometry are labelled separately with their own distance and provenance.
3. Missing recorded sections remain gaps; the application does not replace them with planned points to improve appearance or imply completion.
4. Completion and resemblance metrics describe the observed trace and expose insufficient GPS quality where relevant.
5. External fitness-service integrations require separate implementation and user authorization; they are not implied by GPX export.

## 9 Proposed route generation approach

This is an engineering approach to test, not a guarantee that every requested design can be generated. Begin with a bounded heuristic and a benchmark before committing to a broad catalogue or global launch.

1. **Validate the request and start.** Confirm coverage, profile, template version, allowed distance, and the routable start. Keep the original pin separate from the confirmed network point.
2. **Load the local routing network or provider capability.** Use enough surrounding network for the requested travel budget. Do not equate target route length to a geographic search radius. Bound reachable exploration by graph distance, spatial extent, and computation budget.
3. **Prepare the template.** Store normalised ordered strokes and permitted traversal variants. Perform distance and fitting calculations in a suitable local metric projection, not by treating latitude/longitude degrees as equal metre units.
4. **Search placements.** Explore scale, permitted rotation, translation, entry point, and traversal direction. Include route-to-design and return travel in the budget. For text, maintain the display-orientation constraints.
5. **Route candidate strokes.** Convert important template locations to network anchors and connect them in order under the activity profile. Consider alternatives or corridor penalties when an ordinary shortest path removes an intended stroke.
6. **Assemble the full traversal.** Include access connections, inter-stroke travel, and closure. Validate graph and turn continuity across leg boundaries, including repeated use and grade-separated crossings.
7. **Evaluate and improve.** Measure full-trace resemblance, distance deviation, missing or extra strokes, retracing, and route complexity. Iterate placement and anchor choices under the fixed search budget.
8. **Deduplicate and return.** Keep up to three distinct qualifying candidates, freeze their geometry and metrics, and return clear outcome metadata.

Independently snapping outline points to nearby roads is only an initial heuristic. The connected paths between those points determine the real drawing and must be evaluated together. A routing provider's ability to connect waypoints is not evidence that it can solve the shape-fitting problem by itself.

### Candidate scoring proposal

First reject candidates that violate hard constraints. Then use a versioned normalised objective such as:

`cost = w_shape * full_trace_error + w_distance * relative_distance_error + w_repeat * repeated_distance_ratio + w_connector * connector_distance_ratio + w_complexity * manoeuvre_complexity`

The weights are prototype tuning parameters. They are not product quality percentages. Distance outside the accepted interval remains disqualifying regardless of score.

For full-trace error, test both direction-independent footprint comparison and sequence-aware curve comparison. Include feature checks for distinguishing parts of a shape and for text readability. A low average geometric error alone may miss an absent heart indentation or an extra line through a letter.

For normal results, the route must pass the approved visual-quality threshold as well as the distance gate. If approximate matches are offered, label them separately and keep the full trace visible. The prototype must determine the calibration between internal metrics and human-recognisable results.

## 10 Suggested architecture

Use a responsive TypeScript client with an interactive map, a Go API, a background generation worker, a small persistent job store, and an adapter around the selected routing and geocoding services. This is a proposed fit for a Go-oriented implementation; the product contract does not depend on a particular frontend framework or provider.

For the pilot, an API process and a worker process can share one codebase and database. A database-backed job queue is sufficient if it supports claiming, leases, cancellation, retries, and terminal-state consistency. Introduce additional services only when measured load or provider integration requires them.

| Component            | Responsibility                                                       | Important boundary                            |
| -------------------- | -------------------------------------------------------------------- | --------------------------------------------- |
| Web client           | Input, catalogue, map previews, comparison, local saving, download   | Never invent successful routing data          |
| Planning API         | Validation, guest session, idempotency, job lifecycle, result access | Enforces limits and authorization             |
| Generation worker    | Fitting, network traversal, scoring, deduplication                   | Runs within a fixed time and request budget   |
| Routing adapter      | Profile-aware path computation and available network metadata        | Preserves edge, turn, and geometry semantics  |
| Template catalogue   | Versioned strokes, orientation, supported complexity                 | Curated inputs in the first release           |
| Job and result store | Request snapshots, state, immutable results, expiry                  | Private server records with bounded retention |

Evaluate provider support for activity profiles, restrictions, alternatives, intermediate anchors, geometry, instructions, local coverage, throughput, attribution, and cost before selection. If shape quality requires edge-level corridor control unavailable through a hosted API, the prototype must identify that and evaluate a routable local graph. Do not discover this limitation only after completing the UI.

An LLM is optional for a later natural-language interface. Route geometry, permissions, distance, and continuity must come from deterministic geospatial computation and validated routing data.

## 11 Core data model

| Entity          | Essential fields                                                                                                                              |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| DesignTemplate  | ID, version, category, name, thumbnail, ordered strokes, permitted orientations, stroke joins, complexity, status                             |
| PlanningRequest | Requested pin, confirmed start, activity/profile version, template/version, target metres, tolerance, return setting, request revision        |
| GenerationJob   | ID, guest session owner, idempotency key, input hash, state, stage, timestamps, deadline, attempt counts, cancellation, outcome/error         |
| RouteCandidate  | ID, request/job IDs, immutable route version, ordered geometry, legs, instructions, distances, quality metrics, endpoint, notices, provenance |
| LocalSavedRoute | Local ID, user name, full candidate snapshot, request summary, saved time, schema version                                                     |
| CloudSavedRoute | Later scope: account owner, immutable route reference, name, created/updated time                                                             |
| ShareGrant      | Later scope: owner, route version, opaque token hash, scope, expiry, revoked time                                                             |

Store requested, confirmed, and candidate start positions separately. Store artwork and connector classifications as ranges of traversal occurrences rather than just unique edge IDs, because one edge can be used for different purposes at different times.

Use GeoJSON for geometry interchange. Its positions are longitude then latitude in decimal degrees; adapters must not mix that order with form objects containing named latitude/longitude fields [R2]. Calculate route lengths using metre-based geometry calculations or consistent routed edge lengths, and document the selected source of truth.

## 12 Proposed API contracts

These endpoints define behaviour and boundaries rather than a finished OpenAPI schema.

| Method and path                     | Purpose                                           | Important behaviour                                                                      |
| ----------------------------------- | ------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| GET /api/v1/designs                 | Return published templates                        | Supports category; returns template versions                                             |
| GET /api/v1/coverage                | Describe supported planning coverage              | Used before generation; not proof of local feasibility                                   |
| POST /api/v1/starts/resolve         | Resolve a proposed start for the selected profile | Returns requested point, routable candidate, snap distance, and confirmation requirement |
| POST /api/v1/route-jobs             | Start generation                                  | Requires confirmed start and Idempotency-Key; returns 202 and job ID                     |
| GET /api/v1/route-jobs/{id}         | Read authorized job state                         | Stage, outcome, result IDs, error class, expiry                                          |
| POST /api/v1/route-jobs/{id}/cancel | Cancel an authorized job                          | Idempotent; terminal results cannot be overwritten                                       |
| GET /api/v1/routes/{id}             | Fetch an authorized candidate snapshot            | Geometry, metrics, instructions, versions, provenance                                    |
| GET /api/v1/routes/{id}/export.gpx  | Export exact candidate traversal                  | Planned-track metadata; no activity fabrication                                          |

The browser starts an anonymous session without a sign-up flow. Requests remain bound to that session. Apply cross-site request protections to state-changing requests when using cookie authentication.

An illustrative generation payload is:

```
{
  "start_resolution_id": "start_confirmed_example",
  "activity": "cycling",
  "design": {
    "template_id": "heart",
    "template_version": 1
  },
  "target_distance_m": 15000,
  "distance_tolerance_ratio": 0.1,
  "return_to_start": true,
  "max_candidates": 3,
  "request_revision": 4
}
```

The start resolution is scoped to the same session, profile, and current coverage/version policy. Changing profile or allowing the resolution to expire requires it to be resolved again.

Job states are queued, running, succeeded, no_match, failed, cancelled, and timed_out. State changes use an atomic terminal-state rule so cancellation and completion cannot both publish a result. If fully validated candidates are available at the deadline, the job can succeed with a search-budget-exhausted notice. Use no_match when the configured bounded search completes without a qualifying candidate; this does not prove that no solution exists. Use timed_out when the deadline interrupts unfinished evaluation and no qualifying candidate is available.

Use field-level validation errors for invalid inputs, an unsupported-coverage error for unsupported locations, 429 for rate limits with retry guidance, and distinguish provider failures from a valid unsuccessful search. Polling or event-stream reconnection must preserve request ownership and revision checks.

Cache keys include confirmed start, all route-affecting inputs, profile, template, map data where known, and algorithm version. Reusing spatially nearby work may accelerate search, but cannot return another user's private result directly or silently shift the new start.

## 13 Nonfunctional requirements

The following are proposed pilot targets and must be validated on a documented deployment and workload.

- **Responsiveness:** Input and selection changes respond without waiting for a generation request. Expensive fitting runs outside the browser UI thread.
- **Job acknowledgement:** Aim for 95 percent of valid generation requests to receive an accepted-job response within 1 second under the pilot load.
- **Search latency:** Aim for 95 percent of benchmark jobs to terminate within 30 seconds, with a 60-second maximum search budget. Report the full latency distribution together with result quality and timeout rate.
- **Pilot workload:** Test 20 concurrent active jobs on recorded hardware with a representative distribution of distance, activity, shape, and text requests. External quota constraints are included in the test setup.
- **Consistency:** The route preview, summary, instructions, local snapshot, and GPX export all reference the same route version and ordered geometry.
- **Recovery:** Worker restarts recover or terminate leased jobs without duplicate publication. Provider failure cannot create a false successful result.
- **Location handling:** Collect browser location on demand; protect precise coordinates and traces in transit and at rest where retained. Do not place private coordinates in public URLs or analytics payloads.
- **Browser validation:** Test the planning/export flow in the current desktop Chrome, Safari, and Firefox versions and mobile Safari and Chrome available at release. Document feature-specific fallbacks.
- **Provider obligations:** Before release, verify the selected map and routing providers' applicable usage, attribution, retention, caching, and export requirements against the actual implementation.

## 14 Verification scenarios

| Scenario                                                   | Expected result                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------------------ |
| Heart and 10 km in a known feasible fixture                | A continuous recognisable route within the selected distance range |
| HI or GO with separate strokes                             | All stroke joins and repeated travel visible and counted           |
| Start inside an inaccessible parcel                        | Explicit routable-start proposal or request for a different start  |
| Cycling candidate with steps or a forbidden turn           | Candidate rejected or legally rerouted before scoring              |
| Footpath that is permitted for walking but not cycling     | Activity change produces profile-appropriate results               |
| A bridge visually crosses another road without a junction  | No artificial connection between the two graph edges               |
| River or disconnected component between strokes            | No straight-line shortcut; search continues or returns no match    |
| Exact target missed but candidate remains within tolerance | Actual distance and deviation displayed accurately                 |
| A 10 km request at 10 percent tolerance produces 11.004 km | Rejected as a normal result even if the UI would round to 11.0     |
| Return to start on and off                                 | Endpoint constraints enforced and clearly displayed                |
| Mirror image would give a better text fit                  | Mirrored text rejected                                             |
| Only one distinct candidate qualifies                      | One result shown                                                   |
| Double click, retry, cancellation, and late completion     | One active logical request with a consistent terminal outcome      |
| Inputs change while an earlier job finishes                | Old results do not replace the new request                         |
| Provider outage or quota exhaustion                        | Recoverable dependency error with preserved form state             |
| GPS permission denied or approximate fix                   | Manual start selection remains available                           |
| Local browser storage is unavailable                       | Saving reports failure; selected route and export remain available |
| Guest attempts to fetch another session's route ID         | Access denied without revealing private route content              |
| GPX is exported and reimported                             | Same ordered planned trace, including repeated sections            |
| Future follow mode passes a self-intersection twice        | Progress follows traversal order rather than jumping ahead         |
| Future follow mode loses visible-page updates              | Tracking gap displayed; no synthetic activity segment              |

Test routing geometry with both small controlled graphs and real pilot-area snapshots. Controlled graphs isolate restrictions and ordering bugs; real networks establish whether recognisable designs can actually be found.

## 15 Success metrics and release gates

### Product and operational metrics

Track the proportion of eligible requests returning a qualifying route, generation latency, cancellation and timeout rates, no-match reasons, candidate selection rate, export rate, and provider requests per generation. Break them down by activity, distance, shape versus text, and supported area. Count unsuccessful eligible requests in the denominator.

Measure visual usefulness through explicit candidate feedback and blind review of full-route previews. Export is evidence of planning interest, not proof that the person completed an activity.

### Routing benchmark gate

Prepare at least 50 held-out feasible cases, including at least 20 text and 20 shape cases, distributed across the pilot profiles and distances. A feasible case needs an independently reviewed valid reference traversal on the same network snapshot. Add at least 20 restriction, disconnection, and difficult/no-match cases. Do not label cases impossible merely because the prototype failed to solve them.

Proposed first-release threshold: at least 80 percent of the held-out feasible cases produce a qualifying candidate that at least two of three reviewers can identify from the full trace without being told the intended template. The benchmark must record which design alternatives reviewers choose from and avoid adjusting thresholds using the held-out set. Report performance separately for text and shapes so strong simple shapes cannot hide failed text support.

All returned benchmark routes must satisfy continuity, profile restrictions, fixed-start rules, and distance gates. Invalid traversal is a release blocker regardless of the aggregate success rate. Export round-trip and version-consistency checks must pass for every accepted test route.

### Definition of done for the epic

- Every P0 story has demonstrated acceptance evidence on the target web layouts.
- Both shapes and curated text use the implemented generation service and pass the benchmark gate.
- The core journey from manual start selection to real generated route to GPX export works without an account.
- Unsuccessful searches, cancellation, dependency failures, and stale results have explicit tested outcomes.
- No mock geometry, hidden connector omission, silent start relocation, or straight-line routing fallback remains in production paths.
- Rate limits, expiry, session authorization, observability, and provider configuration are operational.
- Browser and importer checks, product defaults, algorithm version, benchmark results, and unresolved coverage limitations are documented.

## 16 Delivery order and dependencies

| Stage                    | Deliverable                                                                                       | Exit evidence                                                                                               |
| ------------------------ | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| 1 Routing feasibility    | Versioned templates, controlled graph tests, pilot data, one shape and one text traversal         | Continuous road-following examples with honest full-route quality; provider or graph access proven adequate |
| 2 Working vertical slice | Start resolution, activity selection, one template, distance, async job, preview, GPX             | A real end-to-end route generated by the service                                                            |
| 3 First-release planner  | Both catalogues, comparison, refinements, no-match handling, local saving, responsive interaction | P0 story acceptance and route consistency evidence                                                          |
| 4 Pilot hardening        | Benchmark calibration, quotas, cancellations, reliability, location handling, importer checks     | Release gates met with recorded results                                                                     |
| 5 Extensions             | Custom text, foreground following, cloud saving/sharing, activity comparison                      | Separate P1 acceptance evidence and updated coverage constraints                                            |

Routing feasibility should precede large UI investment because the visible interface is not the main uncertainty. Do not assign confident delivery dates or story points to the entire epic before the prototype measures template fit, provider call volume, graph-access needs, and search latency. Estimate the remaining work after Stage 1 using the chosen architecture and observed complexity.

## 17 Decisions to record before implementation

The specification already provides usable defaults. These decisions should be recorded during technical design and prototype review rather than blocking the creation of the initial vertical slice.

- Confirm pilot coverage and the minimum useful catalogue that includes both shapes and text.
- Select routing and geocoding providers or a self-hosted graph after testing required controls and local data quality.
- Calibrate distance bounds, quality bands, acceptable retracing, and computation budgets against representative requests.
- Decide which template-specific features must remain recognisable and how blind review maps to quality labels.
- Confirm the default loop behaviour, start-confirmation UI, guest retention period, and whether all proposed P0 route-detail fields are available from the selected data.
- Record provider cost per typical and difficult generation before enabling broad access or expanding coverage.

## 18 Technical references

These references support the browser and interchange constraints. The product defaults, route-fitting proposal, and release targets are design recommendations in this specification.

- **R1 [W3C Geolocation specification](https://www.w3.org/TR/geolocation/)**, particularly permission, secure-context, and visible-document requirements.
- **R2 [IETF RFC 7946](https://www.rfc-editor.org/rfc/rfc7946)**, GeoJSON coordinate ordering and geographic representation.
- **R3 [Topografix GPX 1.1 schema](https://www.topografix.com/GPX/1/1/)**, track and track-segment representation.

References checked 2 October 2026.