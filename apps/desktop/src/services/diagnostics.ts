import { invoke, isTauri } from "@tauri-apps/api/core";

import { diagnosticFromError, type DiagnosticContext, type DiagnosticEntry } from "./diagnosticsCore";

const diagnosticsKey = "chronicle.diagnostics.v1";

function readBrowserEntries(): DiagnosticEntry[] {
  try { return JSON.parse(localStorage.getItem(diagnosticsKey) ?? "[]") as DiagnosticEntry[]; } catch { return []; }
}

function writeBrowserEntries(entries: DiagnosticEntry[]): void {
  localStorage.setItem(diagnosticsKey, JSON.stringify(entries));
}

export const diagnosticsRepository = {
  async record(error: unknown, context: DiagnosticContext): Promise<DiagnosticEntry> {
    const entry = diagnosticFromError(error, context);
    if (isTauri()) await invoke("append_diagnostic", { entry });
    else writeBrowserEntries([entry, ...readBrowserEntries()].slice(0, 200));
    return entry;
  },

  async list(): Promise<DiagnosticEntry[]> {
    if (isTauri()) return invoke("list_diagnostics");
    return readBrowserEntries();
  },

  async clear(): Promise<void> {
    if (isTauri()) await invoke("clear_diagnostics");
    else localStorage.removeItem(diagnosticsKey);
  },
};

export type { DiagnosticEntry } from "./diagnosticsCore";
