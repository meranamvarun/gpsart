export const screens = ["planner", "catalogue", "generating", "results", "details", "saved", "no-match", "coverage", "error", "login", "auth-pending", "auth-error", "welcome", "account", "screens"] as const;
export type Screen = (typeof screens)[number];
const reviewOnlyScreens: readonly Screen[] = ["welcome", "screens"];

export function currentScreen(hash: string, fallback: string | undefined, designReviewEnabled = false): Screen {
  const requested = hash.replace(/^#\//, "") || fallback || "planner";
  if (!(screens as readonly string[]).includes(requested)) return "planner";
  const screen = requested as Screen;
  return reviewOnlyScreens.includes(screen) && !designReviewEnabled ? "planner" : screen;
}

export function navigate(screen: Screen): void { window.location.hash = `/${screen}`; }
