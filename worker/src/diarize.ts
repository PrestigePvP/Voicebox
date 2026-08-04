import type { Turn } from "./types";

// Response shape verified against a live @cf/deepgram/nova-3 call (Deepgram
// prerecorded format). `metadata.duration` is NOT returned by Workers AI, so
// duration is derived from the last utterance/word end time.
export interface NovaWord {
  word: string;
  punctuated_word?: string;
  start: number;
  end: number;
  confidence?: number;
  speaker?: number;
  speaker_confidence?: number;
}

export interface NovaUtterance {
  id?: string;
  channel?: number;
  start: number;
  end: number;
  confidence?: number;
  speaker?: number;
  transcript: string;
  words?: NovaWord[];
}

export interface NovaResponse {
  results?: {
    channels?: {
      alternatives?: {
        transcript?: string;
        confidence?: number;
        words?: NovaWord[];
      }[];
    }[];
    utterances?: NovaUtterance[];
  };
}

const GAP_BREAK_SEC = 8;

export const mergeUtterances = (
  utterances: NovaUtterance[],
  gapBreakSec: number = GAP_BREAK_SEC,
): Turn[] => {
  const turns: Turn[] = [];

  for (const utt of utterances) {
    const text = utt.transcript.trim();
    if (!text) continue;

    const speaker = utt.speaker ?? 0;
    const last = turns[turns.length - 1];

    if (last && last.speaker === speaker && utt.start - last.end <= gapBreakSec) {
      last.text += ` ${text}`;
      last.end = utt.end;
    } else {
      turns.push({ start: utt.start, end: utt.end, speaker, text });
    }
  }

  return turns;
};

// Fallback when utterances are absent: build pseudo-utterances from
// contiguous same-speaker word runs, then merge as usual.
export const wordsToUtterances = (words: NovaWord[]): NovaUtterance[] => {
  const utterances: NovaUtterance[] = [];

  for (const word of words) {
    const text = word.punctuated_word ?? word.word;
    const speaker = word.speaker ?? 0;
    const last = utterances[utterances.length - 1];

    if (last && (last.speaker ?? 0) === speaker) {
      last.transcript += ` ${text}`;
      last.end = word.end;
    } else {
      utterances.push({ start: word.start, end: word.end, speaker, transcript: text });
    }
  }

  return utterances;
};

export const novaResponseToTurns = (response: NovaResponse): Turn[] => {
  const utterances = response.results?.utterances;
  if (utterances && utterances.length > 0) {
    return mergeUtterances(utterances);
  }

  const words = response.results?.channels?.[0]?.alternatives?.[0]?.words;
  if (words && words.length > 0) {
    return mergeUtterances(wordsToUtterances(words));
  }

  return [];
};
