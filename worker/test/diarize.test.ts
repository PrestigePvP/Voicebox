import { describe, expect, it } from "vitest";
import {
  mergeUtterances,
  novaResponseToTurns,
  wordsToUtterances,
  type NovaUtterance,
  type NovaWord,
} from "../src/diarize";

const utt = (
  start: number,
  end: number,
  speaker: number,
  transcript: string,
): NovaUtterance => ({ start, end, speaker, transcript });

describe("mergeUtterances", () => {
  it("merges consecutive same-speaker utterances", () => {
    const turns = mergeUtterances([
      utt(0, 2, 0, "Hello there."),
      utt(2.5, 5, 0, "How are you?"),
      utt(5.5, 8, 1, "Fine, thanks."),
    ]);
    expect(turns).toEqual([
      { start: 0, end: 5, speaker: 0, text: "Hello there. How are you?" },
      { start: 5.5, end: 8, speaker: 1, text: "Fine, thanks." },
    ]);
  });

  it("breaks a turn on a long gap even for the same speaker", () => {
    const turns = mergeUtterances([
      utt(0, 2, 0, "Before the pause."),
      utt(15, 17, 0, "After the pause."),
    ]);
    expect(turns).toHaveLength(2);
    expect(turns[0].text).toBe("Before the pause.");
    expect(turns[1].start).toBe(15);
  });

  it("respects a custom gap threshold", () => {
    const utterances = [utt(0, 2, 0, "A."), utt(5, 6, 0, "B.")];
    expect(mergeUtterances(utterances, 2)).toHaveLength(2);
    expect(mergeUtterances(utterances, 10)).toHaveLength(1);
  });

  it("skips empty transcripts and defaults missing speaker to 0", () => {
    const turns = mergeUtterances([
      { start: 0, end: 1, transcript: "   " },
      { start: 1, end: 2, transcript: "Hello." },
    ]);
    expect(turns).toEqual([{ start: 1, end: 2, speaker: 0, text: "Hello." }]);
  });
});

describe("wordsToUtterances", () => {
  it("groups contiguous same-speaker word runs", () => {
    const words: NovaWord[] = [
      { word: "hello", punctuated_word: "Hello", start: 0, end: 0.5, speaker: 0 },
      { word: "there", punctuated_word: "there.", start: 0.5, end: 1, speaker: 0 },
      { word: "hi", punctuated_word: "Hi.", start: 1.5, end: 2, speaker: 1 },
    ];
    const utterances = wordsToUtterances(words);
    expect(utterances).toHaveLength(2);
    expect(utterances[0].transcript).toBe("Hello there.");
    expect(utterances[0].speaker).toBe(0);
    expect(utterances[1].transcript).toBe("Hi.");
    expect(utterances[1].speaker).toBe(1);
  });

  it("falls back to the raw word when punctuated_word is absent", () => {
    const utterances = wordsToUtterances([
      { word: "hello", start: 0, end: 0.5, speaker: 0 },
    ]);
    expect(utterances[0].transcript).toBe("hello");
  });
});

describe("novaResponseToTurns", () => {
  it("prefers utterances when present", () => {
    const turns = novaResponseToTurns({
      results: {
        channels: [
          {
            alternatives: [
              { words: [{ word: "ignored", start: 0, end: 1, speaker: 5 }] },
            ],
          },
        ],
        utterances: [utt(0, 2, 1, "From utterances.")],
      },
    });
    expect(turns).toEqual([{ start: 0, end: 2, speaker: 1, text: "From utterances." }]);
  });

  it("falls back to channel words when utterances are absent", () => {
    const turns = novaResponseToTurns({
      results: {
        channels: [
          {
            alternatives: [
              {
                words: [
                  { word: "from", punctuated_word: "From", start: 0, end: 0.5, speaker: 2 },
                  { word: "words", punctuated_word: "words.", start: 0.5, end: 1, speaker: 2 },
                ],
              },
            ],
          },
        ],
      },
    });
    expect(turns).toEqual([{ start: 0, end: 1, speaker: 2, text: "From words." }]);
  });

  it("returns empty for an empty response", () => {
    expect(novaResponseToTurns({})).toEqual([]);
  });
});
