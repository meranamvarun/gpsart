export const screens = ["planner", "catalogue", "generating", "results", "details", "saved", "no-match", "coverage", "error", "login", "auth-pending", "auth-error", "welcome", "account", "screens"] as const;
export type Screen = (typeof screens)[number];

export function currentScreen(hash: string, fallback: string | undefined): Screen {
  const requested = hash.replace(/^#\//, "") || fallback || "planner";
  return (screens as readonly string[]).includes(requested) ? requested as Screen : "planner";
}

export function navigate(screen: Screen): void { window.location.hash = `/${screen}`; }
