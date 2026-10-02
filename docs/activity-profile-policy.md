# Activity profile policy

This document defines GPSART activity profile version 1. It is a routing eligibility policy, not a claim that missing map facts are safe or permitted.

## Published profiles

| Activity | Exact version | Default pace | Access family |
| --- | --- | ---: | --- |
| Walking | `walking-v1` | 720 seconds/km | Foot |
| Running | `running-v1` | 360 seconds/km | Foot |
| Cycling | `cycling-v1` | 240 seconds/km | Cycling |

Every route request records its activity and exact profile version. A candidate copies that identity from its request. A mismatched pair, such as cycling with `walking-v1`, is invalid and is rejected while decoding.

Pace is used only to estimate duration from canonical metres. It cannot change edge or turn eligibility, profile identity, or distance.

## Eligibility rules

| Fact | Foot profiles | Cycling profile |
| --- | --- | --- |
| Access permitted | Eligible | Eligible |
| Access foot-only | Eligible | Ineligible |
| Access bicycle-only | Ineligible | Eligible |
| Access forbidden | Ineligible | Ineligible |
| Steps | Eligible | Ineligible |
| Ferry | Ineligible | Ineligible |
| Motorway | Ineligible | Ineligible |
| Motorroad | Ineligible | Ineligible |
| Traversal against known direction | Ineligible | Ineligible |
| Known forbidden turn | Ineligible | Ineligible |

Surface and elevation do not independently decide legality in version 1. Paved and unpaved edges can be eligible, subject to all other rules.

## Missing metadata

Missing values stay `Unknown`; callers must not replace them with zero, paved, false, bidirectional, or verified access.

- Unknown access, surface, or elevation can remain eligible but produces a corresponding qualification that must travel with the evaluated route.
- Unknown steps, ferry, motorway, or motorroad classification can remain eligible only with the corresponding unknown qualification. Absence is not converted to `false`.
- Unknown direction is ineligible because choosing a traversal direction would require inventing connectivity.
- Unknown turn permission is ineligible because a legal transition cannot be established.

These rules evaluate supplied metadata only. They do not manufacture geometry, connectivity, or access evidence.

## Verification

Run `cargo test -p gpsart-domain --test activity_profiles` and `cargo test -p gpsart-routing --test profile_policy`, or the repository-wide `npm run factory:verify` gate.
