import type { FocusContext } from "./types";

const SYSTEM_PROMPT =
  "You are a text formatter. Take the raw speech-to-text transcription and return it with proper punctuation, capitalization, and paragraph breaks. Do not change the words, only fix formatting. Return only the formatted text.";

const CONTEXT_RULES = `<rules>
- Chat/messaging apps (Slack, Discord, Messages): casual tone, minimal punctuation, no greeting/signature
- Email apps (Mail, Outlook): proper sentences, appropriate formality
- Code editors (VS Code, Xcode): preserve technical terms and code references exactly, this is often used when chatting to an AI assistant about code, so formatting should be clear and precise
- Search fields: concise, no punctuation unless necessary
- General text fields: standard formatting
- If existing text is present, format the new text so it flows naturally as a continuation
</rules>`;

export const buildSystemPrompt = (ctx: FocusContext): string => {
  const hasContext = ctx.appName || ctx.elementRole || ctx.title || ctx.placeholder || ctx.value;
  if (!hasContext) return SYSTEM_PROMPT;
  return `${SYSTEM_PROMPT}\n\n${CONTEXT_RULES}`;
};

export const buildUserMessage = (transcription: string, ctx: FocusContext): string => {
  const contextEntries = Object.entries(ctx)
    .filter(([, value]) => typeof value === "string" && value.length > 0)
    .map(([key, value]) => `  <${key}>${value}</${key}>`);
  const transcriptionEntry = `  <transcription>${transcription}</transcription>`;

  if (contextEntries.length === 0) {
    return transcriptionEntry;
  }
  const contextEntry = `<context>${contextEntries.join("\n")}</context>`;
  return [contextEntry, transcriptionEntry].join("\n\n");
};
