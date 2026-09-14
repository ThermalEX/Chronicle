import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

describe("timeline action column", () => {
  for (const [name, rules, buttonWidth] of [
    ["normal", css.split("@container (max-width: 710px)")[0], 34],
    ["compact", css.split("@container (max-width: 710px)")[1], 32],
  ] as const) {
    it(`${name} reserves room for all five buttons and their gaps`, () => {
      const columns = rules.match(/\.timeline-table-head, \.timeline-table article\s*\{[^}]*grid-template-columns:\s*([^;]+)/)?.[1];
      const actionWidth = Number(columns?.match(/(\d+)px\s*$/)?.[1]);
      expect(actionWidth).toBeGreaterThanOrEqual(buttonWidth * 5 + 5 * 4);
      expect(columns).toContain("minmax(0, 1fr)");
    });
  }
});
