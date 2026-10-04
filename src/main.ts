import "./app.ts";
import { startGuestSessionMaintenance } from "./client/guest-session.ts";

void startGuestSessionMaintenance()
  .then(() => window.dispatchEvent(new Event("gpsart:session-ready")))
  .catch(() => {
    // The planner keeps its explicit unavailable state; never synthesize an authenticated session.
  });
