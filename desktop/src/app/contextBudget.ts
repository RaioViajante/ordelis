// Deterministic counts for synthetic context fixtures. Nothing here is measured by a provider or a tokenizer.

/** Illustrative only. No runtime limit is read or enforced. */
export const EXAMPLE_BUDGET_EST_TOKENS = 6000;
/** Illustrative per-excerpt limit, in characters. */
export const EXCERPT_LIMIT_CHARS = 420;

export interface Counts {
  chars: number;
  bytes: number;
  /** Estimated: ceil(chars / 4). A rule of thumb, not a measurement. */
  estTokens: number;
}

export function countText(text: string): Counts {
  const chars = [...text].length;
  return { chars, bytes: new TextEncoder().encode(text).length, estTokens: Math.ceil(chars / 4) };
}

export interface Excerpt {
  shown: string;
  omittedChars: number;
}

/** Cuts on a character boundary at the limit. `omittedChars` is 0 when nothing was cut. */
export function excerptOf(full: string, limit = EXCERPT_LIMIT_CHARS): Excerpt {
  const chars = [...full];
  if (chars.length <= limit) return { shown: full, omittedChars: 0 };
  return { shown: chars.slice(0, limit).join("").trimEnd(), omittedChars: chars.length - limit };
}

export function sumCounts(all: Counts[]): Counts {
  return all.reduce(
    (a, c) => ({ chars: a.chars + c.chars, bytes: a.bytes + c.bytes, estTokens: a.estTokens + c.estTokens }),
    { chars: 0, bytes: 0, estTokens: 0 },
  );
}

export const fmt = (n: number) => n.toLocaleString("en-US");
