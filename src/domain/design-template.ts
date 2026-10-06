export type DesignCategory = "shape" | "text";

export interface DesignSummary {
  readonly id: string;
  readonly version: number;
  readonly status: "published";
  readonly category: DesignCategory;
  readonly name: string;
  readonly accessibleName: string;
  readonly complexityHint: string;
  readonly previewSvg: string;
  readonly definitionHash: string;
}

export function decodeDesignCatalogue(value: unknown): readonly DesignSummary[] {
  if (!isRecord(value) || !Array.isArray(value.designs)) throw new Error("Invalid design catalogue");
  return Object.freeze(value.designs.map(decodeDesign));
}

function decodeDesign(value: unknown): DesignSummary {
  if (!isRecord(value)
    || typeof value.id !== "string" || value.id.length === 0
    || !Number.isSafeInteger(value.version) || Number(value.version) < 1
    || value.status !== "published"
    || (value.category !== "shape" && value.category !== "text")
    || typeof value.name !== "string" || value.name.length === 0
    || typeof value.accessible_name !== "string" || value.accessible_name.length === 0
    || typeof value.complexity_hint !== "string" || value.complexity_hint.length === 0
    || typeof value.preview_svg !== "string" || !value.preview_svg.startsWith("<svg")
    || typeof value.definition_hash !== "string" || !/^[0-9a-f]{64}$/.test(value.definition_hash)) {
    throw new Error("Invalid design catalogue entry");
  }
  return Object.freeze({
    id: value.id,
    version: Number(value.version),
    status: "published",
    category: value.category,
    name: value.name,
    accessibleName: value.accessible_name,
    complexityHint: value.complexity_hint,
    previewSvg: value.preview_svg,
    definitionHash: value.definition_hash,
  });
}

export type DesignCatalogueLoad =
  | Readonly<{ kind: "available"; designs: readonly DesignSummary[] }>
  | Readonly<{ kind: "unavailable" }>
  | Readonly<{ kind: "invalid_response" }>;

export type DesignCatalogueState = Readonly<{ kind: "loading" }> | DesignCatalogueLoad;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
