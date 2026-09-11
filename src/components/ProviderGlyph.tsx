import type { ProviderId } from "../types/usage";

/** Brand-neutral monochrome glyphs (no trademarked logos), 12×12, currentColor. */
export function ProviderGlyph({ id, size = 12 }: { id: ProviderId; size?: number }) {
  const common = { width: size, height: size, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", strokeWidth: 1.7, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  switch (id) {
    case "claude": // eight-point spark
      return (
        <svg {...common}>
          <path d="M8 2v12M2 8h12M3.8 3.8l8.4 8.4M12.2 3.8l-8.4 8.4" />
        </svg>
      );
    case "codex": // terminal prompt
      return (
        <svg {...common}>
          <path d="M3 4.5l4 3.5-4 3.5M8.5 12h4.5" />
        </svg>
      );
    case "command-code": // command key
      return (
        <svg {...common}>
          <path d="M6 6H4.5a1.5 1.5 0 1 1 1.5-1.5V6zm0 0h4m-4 0v4m4-4V4.5A1.5 1.5 0 1 1 11.5 6H10zm0 0v4m0 0h1.5a1.5 1.5 0 1 1-1.5 1.5V10zm0 0H6m0 0v1.5A1.5 1.5 0 1 1 4.5 10H6z" />
        </svg>
      );
    case "gemini": // four-point star
      return (
        <svg {...common}>
          <path d="M8 2c.6 3.4 2.6 5.4 6 6-3.4.6-5.4 2.6-6 6-.6-3.4-2.6-5.4-6-6 3.4-.6 5.4-2.6 6-6z" />
        </svg>
      );
  }
}
