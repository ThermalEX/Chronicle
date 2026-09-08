import { describe, expect, it } from "vitest";

import { diagnosticFromError, redactDiagnosticText } from "./diagnosticsCore";

describe("diagnostics", () => {
  it("keeps Tauri string failures instead of replacing them with a generic message", () => {
    const entry = diagnosticFromError("WebDAV returned HTTP 401 for PROPFIND", { operation: "同步存档" });

    expect(entry.message).toBe("WebDAV returned HTTP 401 for PROPFIND");
    expect(entry.details).toContain("401");
  });

  it("redacts credentials before errors are persisted or copied", () => {
    const text = "https://alice:secret@example.test/dav?token=abc Authorization: Bearer ghp_example";

    expect(redactDiagnosticText(text)).not.toContain("secret");
    expect(redactDiagnosticText(text)).not.toContain("abc");
    expect(redactDiagnosticText(text)).not.toContain("ghp_example");
  });
});
