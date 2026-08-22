import { describe, expect, it } from "vitest";
import { buildSchemaDiffHighlightSegments } from "@/lib/schema/schemaDiffHighlight";

describe("schema diff character highlighting", () => {
  it("covers disjoint type and default changes without highlighting the shared collation", () => {
    const source = "  `next_step_prompt` longtext COLLATE utf8mb4_unicode_ci,";
    const target = "  `next_step_prompt` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL,";
    const result = buildSchemaDiffHighlightSegments(source, target);

    expect(result.sourceSegments.map((segment) => segment.text).join("")).toBe(source);
    expect(result.targetSegments.map((segment) => segment.text).join("")).toBe(target);
    expect(result.sourceSegments.filter((segment) => segment.changed).map((segment) => segment.text)).toEqual(["longtext"]);
    expect(result.targetSegments.filter((segment) => segment.changed).map((segment) => segment.text)).toEqual(["varchar(255)", " DEFAULT NULL"]);
    expect(result.targetSegments.find((segment) => segment.text.includes("COLLATE"))?.changed).toBe(false);
  });
});
