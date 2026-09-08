export interface DiagnosticContext {
  operation: string;
  archiveId?: string;
  sourceId?: string;
}

export interface DiagnosticEntry extends DiagnosticContext {
  id: string;
  occurredAt: number;
  message: string;
  details: string;
}

const sensitivePatterns: Array<[RegExp, string]> = [
  [/(https?:\/\/[^:\s/@]+:)[^@\s/]+@/gi, "$1[已隐藏]@"],
  [/([?&](?:access_)?token=)[^&\s]+/gi, "$1[已隐藏]"],
  [/(authorization\s*:\s*bearer\s+)[^\s,]+/gi, "$1[已隐藏]"],
  [/\b(?:ghp_[\w-]+|github_pat_[\w-]+)\b/gi, "[已隐藏]"],
  [/("(?:password|token|secret)"\s*:\s*")[^"]*/gi, "$1[已隐藏]"],
];

export function redactDiagnosticText(value: string): string {
  return sensitivePatterns.reduce((result, [pattern, replacement]) => result.replace(pattern, replacement), value);
}

function describeError(error: unknown): string {
  if (error instanceof Error) return [error.message, error.stack].filter(Boolean).join("\n");
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  try { return JSON.stringify(error); } catch { return String(error); }
}

export function diagnosticFromError(error: unknown, context: DiagnosticContext): DiagnosticEntry {
  const details = redactDiagnosticText(describeError(error));
  return {
    id: crypto.randomUUID(),
    occurredAt: Date.now(),
    ...context,
    message: details.split("\n")[0] || "操作失败",
    details: details || "操作失败",
  };
}
