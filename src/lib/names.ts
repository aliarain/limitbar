import type { ProviderId } from "../types/usage";

/** Short labels for the chip. Display names stay in the detail panel. */
export const SHORT_NAME: Record<ProviderId, string> = {
  claude: "Claude",
  codex: "Codex",
  "command-code": "Cmd",
  gemini: "Gemini",
};
