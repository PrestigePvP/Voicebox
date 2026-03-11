import type { FocusContext } from "./types";

const SYSTEM_PROMPT =
  "You are a text formatter. Take the raw speech-to-text transcription and return it with proper punctuation, capitalization, and paragraph breaks. Do not change the words, only fix formatting. Return only the formatted text.";

const CONTEXT_RULES = `<rules>
- Chat/messaging apps (Slack, Discord, Messages): casual tone, minimal punctuation, no greeting/signature
- Email apps (Mail, Outlook): proper sentences, appropriate formality
- Code editors (VS Code, Xcode): preserve technical terms and code references exactly, this is often used when chatting to an AI assistant about code, so formatting should be clear and precise
- Search fields: concise, no punctuation unless necessary
- General text fields: standard formatting
</rules>`;

const EXISTING_TEXT_RULES =
  "The user's text field already contains text (shown in <existingText>). Format the new transcription so it flows naturally as a continuation. Match the tone, style, and capitalization of the existing text. Do NOT repeat or include the existing text in your output — only return the new text, formatted to continue from where the existing text left off.";

const MAX_EXISTING_TEXT = 500;

export const buildSystemPrompt = (ctx: FocusContext): string => {
  const hasContext = ctx.appName || ctx.elementRole || ctx.title || ctx.placeholder;
  const hasExistingText = ctx.value && ctx.value.trim().length > 0;

  let prompt = SYSTEM_PROMPT;
  if (hasContext) prompt += `\n\n${CONTEXT_RULES}`;
  if (hasExistingText) prompt += `\n\n${EXISTING_TEXT_RULES}`;
  return prompt;
};

export const buildUserMessage = (transcription: string, ctx: FocusContext): string => {
  const parts: string[] = [];

  const metaEntries = Object.entries(ctx)
    .filter(([key, value]) => key !== "value" && typeof value === "string" && value.length > 0)
    .map(([key, value]) => `  <${key}>${value}</${key}>`);

  if (metaEntries.length > 0) {
    parts.push(`<context>\n${metaEntries.join("\n")}\n</context>`);
  }

  if (ctx.value && ctx.value.trim().length > 0) {
    const trimmed = ctx.value.length > MAX_EXISTING_TEXT
      ? `...${ctx.value.slice(-MAX_EXISTING_TEXT)}`
      : ctx.value;
    parts.push(`<existingText>${trimmed}</existingText>`);
  }

  parts.push(`<transcription>${transcription}</transcription>`);
  return parts.join("\n\n");
};
