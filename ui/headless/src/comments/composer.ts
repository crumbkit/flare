// The state of one comment or reply field, from the first keystroke to a
// sent post. The composer does no input or output of its own: it calls the
// `send` it is given and reports each change to its listeners.

declare const draftKeyBrand: unique symbol;

/**
 * The idempotency key of one draft. A draft gets its key at its first
 * keystroke and keeps it through every edit and retry until it is sent, so
 * the server can answer a repeated send with the post it already stored.
 */
export type DraftKey = string & { readonly [draftKeyBrand]: true };

export function isDraftKey(raw: string): raw is DraftKey {
  return raw.length > 0;
}

export interface Draft {
  readonly key: DraftKey;
  readonly text: string;
}

/** The outcomes of a send that the server answered. */
export type SendResult<P> =
  | { readonly outcome: "sent"; readonly posted: P }
  | { readonly outcome: "refused" };

/**
 * Posts a draft. It resolves with the server's answer and rejects when the
 * server gave none, such as on a network error, a timeout or a malformed
 * answer.
 */
export type Send<P> = (draft: Draft) => Promise<SendResult<P>>;

/** Why a send failed. Only a send with no answer may be retried. */
export type Failure = "no-answer" | "refused";

export type ComposerState<P> =
  | { readonly status: "idle" }
  | { readonly status: "composing"; readonly draft: Draft }
  | { readonly status: "sending"; readonly draft: Draft }
  | { readonly status: "failed"; readonly draft: Draft; readonly failure: Failure }
  | { readonly status: "sent"; readonly posted: P };

/** Why a composer cannot send now. */
export type Blocker = "empty" | "too-long" | "sending" | "refused";

function textBlocker(text: string, maxLength: number): Blocker | undefined {
  if (text.trim() === "") return "empty";
  // A string's length counts UTF-16 code units; the limit counts characters.
  return [...text].length > maxLength ? "too-long" : undefined;
}

export function blocker<P>(state: ComposerState<P>, maxLength: number): Blocker | undefined {
  switch (state.status) {
    case "idle":
    case "sent":
      return "empty";
    case "sending":
      return "sending";
    case "composing":
      return textBlocker(state.draft.text, maxLength);
    case "failed":
      return state.failure === "refused" ? "refused" : textBlocker(state.draft.text, maxLength);
  }
}

export function canRetry<P>(state: ComposerState<P>): boolean {
  return state.status === "failed" && state.failure === "no-answer";
}

export interface ComposerOptions<P> {
  readonly send: Send<P>;
  readonly newKey: () => DraftKey;
  /** The most characters a post may hold. */
  readonly maxLength: number;
}

export interface Composer<P> {
  getState(): ComposerState<P>;
  /** Calls `listener` after each change, until the returned function runs. */
  subscribe(listener: () => void): () => void;
  /** Replaces the field's text. A sending composer ignores it. */
  edit(text: string): void;
  /** Sends the draft unless a blocker stops it; a failed draft is resent with its key. */
  submit(): Promise<void>;
}

function edited<P>(
  state: ComposerState<P>,
  text: string,
  newKey: () => DraftKey,
): ComposerState<P> {
  switch (state.status) {
    case "sending":
      return state;
    case "idle":
    case "sent":
      return text === "" ? state : { status: "composing", draft: { key: newKey(), text } };
    case "composing":
      return text === "" ? { status: "idle" } : { ...state, draft: { ...state.draft, text } };
    case "failed": {
      if (text === "") return { status: "idle" };
      const draft = { ...state.draft, text };
      // An edit after a refusal cannot be sent, so the failure stays shown.
      return state.failure === "refused" ? { ...state, draft } : { status: "composing", draft };
    }
  }
}

export function createComposer<P>(options: ComposerOptions<P>): Composer<P> {
  let state: ComposerState<P> = { status: "idle" };
  const listeners = new Set<() => void>();
  // Reading the state through a function drops the narrowing that the
  // checks before an await left on it, since a listener may change it.
  const getState = (): ComposerState<P> => state;

  function set(next: ComposerState<P>): void {
    if (next === state) return;
    state = next;
    for (const listener of listeners) listener();
  }

  return {
    getState,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    edit(text) {
      set(edited(state, text, options.newKey));
    },
    async submit() {
      if (state.status === "idle" || state.status === "sent" || state.status === "sending") return;
      if (blocker(state, options.maxLength) !== undefined) return;
      const sending: ComposerState<P> = { status: "sending", draft: state.draft };
      set(sending);
      let next: ComposerState<P>;
      try {
        const result = await options.send(sending.draft);
        next =
          result.outcome === "sent"
            ? { status: "sent", posted: result.posted }
            : { status: "failed", draft: sending.draft, failure: "refused" };
      } catch {
        // A send with no answer is the failure the composer exists to show,
        // so the composer reports it as state and does not rethrow it.
        next = { status: "failed", draft: sending.draft, failure: "no-answer" };
      }
      if (getState() === sending) set(next);
    },
  };
}
