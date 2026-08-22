import { diffChars } from "diff";

export interface SchemaDiffHighlightSegment {
  text: string;
  changed: boolean;
}

export interface SchemaDiffHighlightSegments {
  sourceSegments: SchemaDiffHighlightSegment[];
  targetSegments: SchemaDiffHighlightSegment[];
}

export function buildSchemaDiffHighlightSegments(source: string, target: string): SchemaDiffHighlightSegments {
  const sourceSegments: SchemaDiffHighlightSegment[] = [];
  const targetSegments: SchemaDiffHighlightSegment[] = [];

  for (const change of diffChars(source, target)) {
    if (change.removed) {
      sourceSegments.push({ text: change.value, changed: true });
    } else if (change.added) {
      targetSegments.push({ text: change.value, changed: true });
    } else {
      sourceSegments.push({ text: change.value, changed: false });
      targetSegments.push({ text: change.value, changed: false });
    }
  }

  return { sourceSegments, targetSegments };
}
