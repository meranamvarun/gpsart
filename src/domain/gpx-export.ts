import type { RouteSnapshot } from "./route-snapshot.ts";

function xml(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&apos;");
}

function filenamePart(value: string): string {
  const sanitized = value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40).replace(/-+$/g, "");
  return sanitized || "route";
}

export function routeExportFilename(snapshot: RouteSnapshot): string {
  return `gpsart-${filenamePart(snapshot.versions.templateId)}-${snapshot.id}-v${snapshot.version}.gpx`;
}

export function exportRouteSnapshotGpx(snapshot: RouteSnapshot): string {
  const routeId = xml(snapshot.id);
  const points = snapshot.trace.map(({ latitude, longitude }) => `      <trkpt lat="${latitude}" lon="${longitude}"></trkpt>`).join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="GPSART" xmlns="http://www.topografix.com/GPX/1/1" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.topografix.com/GPX/1/1 https://www.topografix.com/GPX/1/1/gpx.xsd">
  <metadata>
    <name>GPSART planned route ${routeId}</name>
    <desc>Planned route from immutable snapshot route-candidate-snapshot-v1; not a recorded activity.</desc>
  </metadata>
  <trk>
    <name>GPSART ${xml(snapshot.versions.templateId)}@${snapshot.versions.template} planned route</name>
    <desc>Exact ordered traversal including connectors and retraced sections.</desc>
    <src>GPSART route ${routeId}; candidate version ${snapshot.version}; profile ${xml(snapshot.versions.profile)}; routing data ${xml(snapshot.versions.routingData)}</src>
    <type>planned-route</type>
    <trkseg>
${points}
    </trkseg>
  </trk>
</gpx>
`;
}

export function downloadRouteSnapshotGpx(snapshot: RouteSnapshot, documentRoot: Document = document): void {
  const blob = new Blob([exportRouteSnapshotGpx(snapshot)], { type: "application/gpx+xml;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = documentRoot.createElement("a");
  anchor.href = url;
  anchor.download = routeExportFilename(snapshot);
  anchor.rel = "noopener";
  anchor.click();
  URL.revokeObjectURL(url);
}
