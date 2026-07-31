import { describe, expect, it } from "vitest";
import {
  buildEnrichMessages,
  emptyEnrichResult,
  parseEnrichResponse,
  sampleTurns,
} from "../src/meeting-prompt";
import type { Turn } from "../src/types";

const turn = (speaker: number, text: string, start = 0, end = 1): Turn => ({
  start,
  end,
  speaker,
  text,
});

const words = (n: number): string => Array.from({ length: n }, (_, i) => `w${i}`).join(" ");

describe("sampleTurns", () => {
  it("keeps everything under budget as a single block", () => {
    const turns = [turn(0, "short one"), turn(1, "short two")];
    const { blocks, sampled } = sampleTurns(turns, 100);
    expect(sampled).toBe(false);
    expect(blocks).toEqual([turns]);
  });

  it("returns no blocks for empty input", () => {
    expect(sampleTurns([], 100).blocks).toEqual([]);
  });

  it("samples down to roughly the word budget when over", () => {
    const turns = Array.from({ length: 100 }, (_, i) => turn(i % 2, words(50), i, i + 1));
    const { blocks, sampled } = sampleTurns(turns, 1000);
    expect(sampled).toBe(true);

    const kept = blocks.flat();
    const keptWords = kept.reduce((a, t) => a + t.text.split(/\s+/).length, 0);
    expect(keptWords).toBeLessThanOrEqual(1100);
    expect(keptWords).toBeGreaterThan(500);

    // Front-weighted: first turn kept, last turn kept
    expect(kept[0]).toBe(turns[0]);
    expect(kept[kept.length - 1]).toBe(turns[turns.length - 1]);
    // Middle was thinned into gaps → more than one block
    expect(blocks.length).toBeGreaterThan(1);
  });

  it("preserves turn order across blocks", () => {
    const turns = Array.from({ length: 60 }, (_, i) => turn(0, words(40), i, i + 1));
    const kept = sampleTurns(turns, 800).blocks.flat();
    const starts = kept.map((t) => t.start);
    expect([...starts].sort((a, b) => a - b)).toEqual(starts);
  });
});

describe("buildEnrichMessages", () => {
  it("renders speaker-labeled lines inside transcript tags", () => {
    const messages = buildEnrichMessages([[turn(0, "Hello."), turn(1, "Hi.")]], false);
    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe("system");
    expect(messages[1].content).toContain("<transcript>");
    expect(messages[1].content).toContain("[Speaker 0] Hello.");
    expect(messages[1].content).toContain("[Speaker 1] Hi.");
    expect(messages[1].content).not.toContain("omitted");
  });

  it("marks gaps between blocks when sampled", () => {
    const messages = buildEnrichMessages([[turn(0, "Start.")], [turn(1, "End.")]], true);
    expect(messages[1].content).toContain("(middle portion of the meeting omitted)");
    expect(messages[1].content).toContain("some middle portions are omitted");
  });
});

describe("parseEnrichResponse", () => {
  it("parses a clean JSON object", () => {
    const result = parseEnrichResponse(
      '{"title": "Roadmap Sync", "summary": "They planned.", "speakers": {"0": "Mark", "1": null}}',
      [0, 1],
    );
    expect(result).toEqual({
      title: "Roadmap Sync",
      summary: "They planned.",
      speakers: { "0": "Mark", "1": null },
    });
  });

  it("strips markdown fences and surrounding prose", () => {
    const raw = 'Sure! Here is the JSON:\n```json\n{"title": "T", "summary": "S", "speakers": {"0": "Ana"}}\n```\nDone.';
    const result = parseEnrichResponse(raw, [0]);
    expect(result?.title).toBe("T");
    expect(result?.speakers["0"]).toBe("Ana");
  });

  it("returns null for garbage", () => {
    expect(parseEnrichResponse("no json here", [0])).toBeNull();
    expect(parseEnrichResponse("{broken json", [0])).toBeNull();
  });

  it("fills missing speaker ids with null and rejects invented labels", () => {
    const result = parseEnrichResponse(
      '{"title": "T", "summary": "S", "speakers": {"0": "Speaker 0", "2": "Zoe"}}',
      [0, 1, 2],
    );
    expect(result?.speakers).toEqual({ "0": null, "1": null, "2": "Zoe" });
  });

  it("tolerates missing title/summary", () => {
    const result = parseEnrichResponse('{"speakers": {}}', [0]);
    expect(result).toEqual({ title: null, summary: null, speakers: { "0": null } });
  });
});

describe("emptyEnrichResult", () => {
  it("builds a null map for all speakers", () => {
    expect(emptyEnrichResult([0, 1])).toEqual({
      title: null,
      summary: null,
      speakers: { "0": null, "1": null },
    });
  });
});
