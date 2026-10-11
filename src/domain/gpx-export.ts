import type { RouteSnapshot } from "./route-snapshot.ts";

function xml(value: string): string {
  const valid = [...value].map((character) => {
    const codePoint = character.codePointAt(0)!;
    const allowed = codePoint === 0x09 || codePoint === 0x0a || codePoint === 0x0d
      || (codePoint >= 0x20 && codePoint <= 0xd7ff)
      || (codePoint >= 0xe000 && codePoint <= 0xfffd)
      || (codePoint >= 0x10000 && codePoint <= 0x10ffff);
    return allowed ? character : "\uFFFD";
  }).join("");
  return valid.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&apos;");
}

function coordinate(value: number): string {
  const rendered = String(value);
  const exponential = /^(-?)(\d+)(?:\.(\d+))?[eE]([+-]?\d+)$/.exec(rendered);
  if (!exponential) return rendered;
  const [, sign, integer, fraction = "", exponentText] = exponential;
  const digits = `${integer}${fraction}`;
  const decimalIndex = integer.length + Number(exponentText);
  if (decimalIndex <= 0) return `${sign}0.${"0".repeat(-decimalIndex)}${digits}`;
  if (decimalIndex >= digits.length) return `${sign}${digits}${"0".repeat(decimalIndex - digits.length)}`;
  return `${sign}${digits.slice(0, decimalIndex)}.${digits.slice(decimalIndex)}`;
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
  const copyright = snapshot.provenance.attribution === undefined
    ? ""
    : `    <copyright author="${xml(snapshot.provenance.attribution)}"></copyright>\n`;
  const points = snapshot.trace.map(({ latitude, longitude }) => `      <trkpt lat="${coordinate(latitude)}" lon="${coordinate(longitude)}"></trkpt>`).join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="GPSART" xmlns="http://www.topografix.com/GPX/1/1" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.topografix.com/GPX/1/1 https://www.topografix.com/GPX/1/1/gpx.xsd">
  <metadata>
    <name>GPSART planned route ${routeId}</name>
    <desc>Planned route from immutable snapshot route-candidate-snapshot-v1; not a recorded activity.</desc>
${copyright}  </metadata>
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
