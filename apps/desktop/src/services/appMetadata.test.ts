import { expect, it } from "vitest";
import packageJson from "../../package.json";
import { appMetadata } from "./appMetadata";

it("exposes Chronicle branding from the build version", () => {
  expect(appMetadata).toEqual({
    name: "Chronicle",
    author: "ThermalEX",
    version: packageJson.version,
  });
});
