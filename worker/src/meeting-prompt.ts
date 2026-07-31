import type { Turn } from "./types";

const MAX_SAMPLE_WORDS = 12000;
const HEAD_FRACTION = 0.5;
const TAIL_FRACTION = 0.2;

const ENRICH_SYSTEM_PROMPT = `You analyze a meeting transcript. Speakers are labeled Speaker 0, Speaker 1, etc.
Respond with ONLY a JSON object, no markdown fences, exactly this shape:
{"title": "...", "summary": "...", "speakers": {"0": "Name or null", "1": null}}
Rules:
- title: at most 8 words describing the meeting topic.
- summary: 2-4 plain-text sentences covering the key points and decisions.
- speakers: one entry per speaker id. Give a real name ONLY when the transcript
  clearly reveals it (they introduce themselves or another speaker addresses them
  by name). Otherwise use null. Never guess or invent names.`;

export interface EnrichResult {
  title: string | null;
  summary: string | null;
  speakers: Record<string, string | null>;
}

const wordCount = (text: string): number => text.split(/\s+/).filter(Boolean).length;

// Sample turns down to a word budget, front-weighted: names are usually
// revealed in introductions and sign-offs. Returns contiguous blocks so the
// message builder can mark the gaps between them.
export const sampleTurns = (
  turns: Turn[],
  maxWords: number = MAX_SAMPLE_WORDS,
): { blocks: Turn[][]; sampled: boolean } => {
  const counts = turns.map((t) => wordCount(t.text));
  const total = counts.reduce((a, b) => a + b, 0);

  if (total <= maxWords) {
    return { blocks: turns.length > 0 ? [turns] : [], sampled: false };
  }

  const headBudget = maxWords * HEAD_FRACTION;
  const tailBudget = maxWords * TAIL_FRACTION;
  const middleBudget = maxWords - headBudget - tailBudget;

  let headEnd = 0;
  let used = 0;
  while (headEnd < turns.length && used + counts[headEnd] <= headBudget) {
    used += counts[headEnd];
    headEnd++;
  }

  let tailStart = turns.length;
  used = 0;
  while (tailStart > headEnd && used + counts[tailStart - 1] <= tailBudget) {
    used += counts[tailStart - 1];
    tailStart--;
  }

  const middle = turns.slice(headEnd, tailStart);
  const middleCounts = counts.slice(headEnd, tailStart);
  const middleTotal = middleCounts.reduce((a, b) => a + b, 0);
  const keepRatio = middleTotal > 0 ? middleBudget / middleTotal : 0;

  const middleBlocks: Turn[][] = [];
  let acc = 0;
  let current: Turn[] = [];
  for (let i = 0; i < middle.length; i++) {
    acc += keepRatio;
    if (acc >= 1) {
      acc -= 1;
      current.push(middle[i]);
    } else if (current.length > 0) {
      middleBlocks.push(current);
      current = [];
    }
  }
  if (current.length > 0) middleBlocks.push(current);

  const blocks: Turn[][] = [];
  if (headEnd > 0) blocks.push(turns.slice(0, headEnd));
  blocks.push(...middleBlocks);
  if (tailStart < turns.length) blocks.push(turns.slice(tailStart));

  return { blocks, sampled: true };
};

export const buildEnrichMessages = (
  blocks: Turn[][],
  sampled: boolean,
): { role: string; content: string }[] => {
  const rendered = blocks
    .map((block) => block.map((t) => `[Speaker ${t.speaker}] ${t.text}`).join("\n"))
    .join("\n(middle portion of the meeting omitted)\n");

  const note = sampled ? "\nNote: long meeting — some middle portions are omitted." : "";

  return [
    { role: "system", content: ENRICH_SYSTEM_PROMPT },
    { role: "user", content: `<transcript>\n${rendered}\n</transcript>${note}` },
  ];
};

const cleanName = (value: unknown): string | null => {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!trimmed || trimmed.toLowerCase() === "null") return null;
  if (/^speaker\s*\d+$/i.test(trimmed)) return null;
  if (trimmed.length > 60) return null;
  return trimmed;
};

const cleanText = (value: unknown, maxLen: number): string | null => {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  return trimmed.length > maxLen ? trimmed.slice(0, maxLen) : trimmed;
};

export const parseEnrichResponse = (
  raw: string,
  speakerIds: number[],
): EnrichResult | null => {
  const withoutFences = raw.replace(/```(?:json)?/gi, "");
  const start = withoutFences.indexOf("{");
  const end = withoutFences.lastIndexOf("}");
  if (start === -1 || end <= start) return null;

  let parsed: unknown;
  try {
    parsed = JSON.parse(withoutFences.slice(start, end + 1));
  } catch {
    return null;
  }

  if (typeof parsed !== "object" || parsed === null) return null;
  const obj = parsed as Record<string, unknown>;

  const rawSpeakers =
    typeof obj.speakers === "object" && obj.speakers !== null
      ? (obj.speakers as Record<string, unknown>)
      : {};

  const speakers: Record<string, string | null> = {};
  for (const id of speakerIds) {
    speakers[String(id)] = cleanName(rawSpeakers[String(id)]);
  }

  return {
    title: cleanText(obj.title, 120),
    summary: cleanText(obj.summary, 2000),
    speakers,
  };
};

export const emptyEnrichResult = (speakerIds: number[]): EnrichResult => ({
  title: null,
  summary: null,
  speakers: Object.fromEntries(speakerIds.map((id) => [String(id), null])),
});
