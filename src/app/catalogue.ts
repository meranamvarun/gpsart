export type DesignCategory = "Shapes" | "Text";

export interface DesignTemplatePreview {
  readonly name: string;
  readonly icon: string | null;
  readonly complexity: "Easy" | "Moderate" | "Complex";
}

export const shapePreviews: readonly DesignTemplatePreview[] = [
  { name: "Heart", icon: "heart", complexity: "Easy" },
  { name: "Circle", icon: "circle", complexity: "Easy" },
  { name: "Triangle", icon: "triangle", complexity: "Easy" },
  { name: "Star", icon: "star", complexity: "Complex" },
  { name: "Square", icon: "square", complexity: "Easy" },
];

export const textPreviews: readonly DesignTemplatePreview[] = [
  { name: "HI", icon: null, complexity: "Moderate" },
  { name: "GO", icon: null, complexity: "Moderate" },
  { name: "RUN", icon: null, complexity: "Complex" },
];

export function previewsFor(category: DesignCategory): readonly DesignTemplatePreview[] {
  return category === "Shapes" ? shapePreviews : textPreviews;
}
