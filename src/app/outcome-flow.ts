import type { PlannerWorkflowSnapshot, WorkflowIssue } from "../client/planner-workflow.ts";
import type { RouteJobOutcomeCode } from "../domain/job-status.ts";
import type { Screen } from "./router.ts";

export type OutcomePresentation = Readonly<{
  title: string;
  explanation: string;
  suggestions: readonly string[];
  feasibility: "not_evaluated";
  retryable: boolean;
}>;

const presentations: Readonly<Record<RouteJobOutcomeCode, OutcomePresentation>> = {
  succeeded: {
    title: "A validated route was found.",
    explanation: "The service returned one or more validated route references for this exact request.",
    suggestions: [], feasibility: "not_evaluated", retryable: false,
  },
  unsupported_coverage: {
    title: "Outside verified pilot coverage.",
    explanation: "The service checked this start against the current coverage policy and did not accept it for this search.",
    suggestions: ["Choose another exact start inside the published pilot area."], feasibility: "not_evaluated", retryable: false,
  },
  no_routable_access: {
    title: "No verified road access near this start.",
    explanation: "The service did not verify a routable network point for this activity near the requested start.",
    suggestions: ["Choose another exact start.", "Choose a different activity profile."], feasibility: "not_evaluated", retryable: false,
  },
  disconnected_network: {
    title: "The checked roads did not connect.",
    explanation: "The bounded search found no qualifying continuous traversal for this exact request.",
    suggestions: ["Choose another start.", "Change the target distance or design."], feasibility: "not_evaluated", retryable: false,
  },
  insufficient_visual_match: {
    title: "No qualifying visual match was found.",
    explanation: "The bounded search completed without a route that met the configured visual-match requirement.",
    suggestions: ["Choose a simpler design.", "Change the target distance or allowed difference."], feasibility: "not_evaluated", retryable: false,
  },
  outside_distance: {
    title: "No result was inside your distance interval.",
    explanation: "The bounded search returned no qualifying route inside the unrounded distance interval you requested.",
    suggestions: ["Change the target distance.", "Widen the allowed difference."], feasibility: "not_evaluated", retryable: false,
  },
  exhausted_budget: {
    title: "The bounded search limit was reached.",
    explanation: "The configured search budget ended without a qualifying result; this does not prove that no route exists.",
    suggestions: ["Choose a different design, start, or distance before starting a new search."], feasibility: "not_evaluated", retryable: false,
  },
  provider_unavailable: {
    title: "The routing dependency was unavailable.",
    explanation: "This completed attempt could not use the routing dependency. Its original inputs and outcome remain recorded.",
    suggestions: ["Retry the unchanged inputs as a new request revision after the advised delay."], feasibility: "not_evaluated", retryable: true,
  },
  attempts_exhausted: {
    title: "This request used all configured attempts.",
    explanation: "The worker stopped after the configured attempt limit without publishing a qualifying route.",
    suggestions: ["Review the original inputs before choosing whether to start a new request."], feasibility: "not_evaluated", retryable: false,
  },
  cancelled: {
    title: "Search cancelled.",
    explanation: "The service confirmed cancellation and did not publish a route result for this request.",
    suggestions: ["Review or edit the request before starting again."], feasibility: "not_evaluated", retryable: false,
  },
  timed_out: {
    title: "The search deadline was reached.",
    explanation: "The bounded job ended before a qualifying result was completed; this does not prove that no route exists.",
    suggestions: ["Review the start, design, distance, and tolerance before starting a new request."], feasibility: "not_evaluated", retryable: false,
  },
  legacy_unknown: {
    title: "The stored outcome is unavailable.",
    explanation: "This historical request ended without a classification that the current client can present safely.",
    suggestions: ["Review the original inputs before starting a new request."], feasibility: "not_evaluated", retryable: false,
  },
};

export function outcomePresentation(code: RouteJobOutcomeCode): OutcomePresentation {
  return presentations[code];
}

function screenForIssue(issue: WorkflowIssue | null): Screen {
  if (issue === "unsupported_coverage" || issue === "no_routable_access") return "coverage";
  if ([
    "provider_failure", "storage_unavailable", "network_unavailable", "job_storage_unavailable",
    "csrf_unavailable", "invalid_response",
  ].includes(issue ?? "")) return "error";
  return "planner";
}

export function screenForWorkflow(snapshot: PlannerWorkflowSnapshot): Screen {
  if (snapshot.phase === "polling") return "generating";
  if (snapshot.phase === "cancelled" || snapshot.phase === "stale") return "planner";
  if (snapshot.phase === "error") return screenForIssue(snapshot.issue);
  if (snapshot.phase !== "terminal" || !snapshot.status?.outcome) return "planner";
  switch (snapshot.status.outcome.code) {
    case "succeeded": return "results";
    case "unsupported_coverage":
    case "no_routable_access": return "coverage";
    case "disconnected_network":
    case "insufficient_visual_match":
    case "outside_distance":
    case "exhausted_budget": return "no-match";
    case "cancelled": return "planner";
    case "provider_unavailable":
    case "attempts_exhausted":
    case "timed_out":
    case "legacy_unknown": return "error";
  }
}
