import type { DesignCatalogueState, DesignSummary } from "../domain/design-template.ts";

export type DesignCategory = "Shapes" | "Text";

export interface DesignTemplatePreview {
  readonly id: string;
  readonly version: number;
  readonly name: string;
  readonly icon: string | null;
  readonly complexity: "Easy" | "Moderate" | "Complex";
}

export interface PresentedDesignTemplate extends DesignTemplatePreview {
  readonly accessibleName: string;
  readonly enabled: boolean;
  readonly statusReason: string | null;
}

export const shapePreviews: readonly DesignTemplatePreview[] = [
  { id: "heart", version: 1, name: "Heart", icon: "heart", complexity: "Easy" },
  { id: "circle", version: 1, name: "Circle", icon: "circle", complexity: "Easy" },
  { id: "triangle", version: 1, name: "Triangle", icon: "triangle", complexity: "Easy" },
  { id: "star", version: 1, name: "Star", icon: "star", complexity: "Complex" },
  { id: "square", version: 1, name: "Square", icon: "square", complexity: "Easy" },
];

export const textPreviews: readonly DesignTemplatePreview[] = [
  { id: "HI", version: 1, name: "HI", icon: null, complexity: "Moderate" },
  { id: "GO", version: 1, name: "GO", icon: null, complexity: "Moderate" },
  { id: "RUN", version: 1, name: "RUN", icon: null, complexity: "Complex" },
];

function publishedMatch(preview: DesignTemplatePreview, category: DesignCategory, designs: readonly DesignSummary[]): DesignSummary | undefined {
  const expectedCategory = category === "Shapes" ? "shape" : "text";
  return designs.find(({ id, version, category: actualCategory, name }) => id === preview.id
    && version === preview.version
    && actualCategory === expectedCategory
    && name === preview.name);
}

export function previewsFor(category: DesignCategory, state: DesignCatalogueState): readonly PresentedDesignTemplate[] {
  const previews = category === "Shapes" ? shapePreviews : textPreviews;
  return Object.freeze(previews.map((preview) => {
    const published = state.kind === "available" ? publishedMatch(preview, category, state.designs) : undefined;
    const statusReason = published ? null
      : state.kind === "loading" ? `${preview.name} availability is still loading.`
        : state.kind === "unavailable" ? `${preview.name} is unavailable while the design catalogue cannot be reached.`
          : state.kind === "invalid_response" ? `${preview.name} is unavailable because the design catalogue response was invalid.`
            : `${preview.name} is not available because it has no reviewed legal routing fixture.`;
    return Object.freeze({
      ...preview,
      accessibleName: published?.accessibleName ?? `${preview.name} route design`,
      enabled: published !== undefined,
      statusReason,
    });
  }));
}

export function isDesignEnabled(name: string, state: DesignCatalogueState): boolean {
  return [...previewsFor("Shapes", state), ...previewsFor("Text", state)]
    .some((preview) => preview.name === name && preview.enabled);
}
