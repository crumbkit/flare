// The words and the keyboard rule of the comment parts, as mockup
// LRJsZhYqKPHcug6pQKw8dA version 1795398322-8ae4 draws them. The messages
// for a full flare, an ended session and invalid text, which the mockup does
// not draw, follow the wording of its refusal. Each binding gives these
// strings to the accessible names, live regions and alerts it renders, so
// every front end says the same thing.
import type { Failure } from "./composer.ts";
import type { Target, Thread } from "./thread.ts";

export const sectionHeading = "Comments";
export const loadingStatus = "Loading comments";
export const loadFailedMessage =
  "The comments did not load because Flare did not answer. The page above is unaffected.";
export const retryText = "Retry";
/** The line a deleted comment's card shows in place of its author, time and text. */
export const deletedText = "Its author deleted this comment.";
/** The hint under the page field while it holds text. */
export const sendHint = "Ctrl or ⌘ + Enter to send";

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** The count beside the heading, such as "2 comments, 2 replies". */
export function threadSummary(thread: Thread): string {
  if (thread.length === 0) return "None yet";
  const replies = thread.reduce((sum, c) => sum + c.replies.length, 0);
  const comments = plural(thread.length, "comment", "comments");
  return replies === 0 ? comments : `${comments}, ${plural(replies, "reply", "replies")}`;
}

function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] ?? name;
}

/** The line beside Comment that says who can read a comment on the page. */
export function audienceLine(owner: string): string {
  return `${firstName(owner)} and everyone this page is shared with can read it.`;
}

export function fieldLabel(target: Target): string {
  switch (target.kind) {
    case "comment":
      return "Comment on the whole page";
    case "reply":
      return `Reply to ${firstName(target.to.name)}`;
  }
}

export function placeholder(target: Target, threadIsEmpty: boolean): string {
  switch (target.kind) {
    case "comment":
      return threadIsEmpty ? "Be the first to comment." : "Write a comment.";
    case "reply":
      return fieldLabel(target);
  }
}

export function submitText(target: Target, sending: boolean): string {
  if (sending) return "Sending…";
  switch (target.kind) {
    case "comment":
      return "Comment";
    case "reply":
      return "Reply";
  }
}

/**
 * The first character of `text` that flare-server refuses, a control
 * character other than tab, line feed and carriage return, with its position
 * counted in characters from 1.
 */
function refusedCharacter(text: string): { codePoint: string; position: number } | undefined {
  let position = 0;
  for (const character of text) {
    position += 1;
    if (/\p{Cc}/u.test(character) && !"\t\n\r".includes(character)) {
      const hex = (character.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, "0");
      return { codePoint: `U+${hex}`, position };
    }
  }
  return undefined;
}

function invalidMessage(target: Target, text: string): string {
  const found = refusedCharacter(text);
  const what =
    found === undefined
      ? "a control character"
      : `the control character ${found.codePoint} at character ${found.position}`;
  return `Your ${target.kind} was not sent because it holds ${what}, which Flare does not store. Remove it and send again.`;
}

/** What a field says when its send of `text` failed. */
export function failureMessage(
  target: Target,
  failure: Failure,
  owner: string,
  text: string,
): string {
  switch (failure) {
    case "no-answer":
      return `Your ${target.kind} was not sent because Flare did not answer. Your text is still in the field.`;
    case "refused":
      return `You can no longer comment here, because you no longer have access to this page. Copy your text and ask ${owner} for access.`;
    case "full":
      return `Your ${target.kind} was not sent because this page already holds 2,000 comments and replies, the most a page may hold. Copy your text to keep it.`;
    case "signed-out":
      return `Your ${target.kind} was not sent because your session ended. Copy your text, then reload the page to sign in again.`;
    case "invalid":
      return invalidMessage(target, text);
  }
}

export function failureAction(failure: Failure): string {
  return failure === "no-answer" ? retryText : "Copy text";
}

export function sentAnnouncement(target: Target): string {
  switch (target.kind) {
    case "comment":
      return "Comment sent";
    case "reply":
      return "Reply sent";
  }
}

/** Why Send is disabled for a text of `length` characters over `maxLength`. */
export function tooLongReason(length: number, maxLength: number): string {
  const over = plural(length - maxLength, "character", "characters");
  return `Your text is ${over} over the limit of ${maxLength.toLocaleString("en-GB")} characters.`;
}

/** The parts of a key press the keyboard rule reads. */
export interface KeyPress {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly isComposing: boolean;
}

/**
 * Whether a key press in a field sends it. In the page field Enter adds a
 * line and Ctrl or Command with Enter sends. In the one-line reply field
 * Enter sends. The Enter that ends an input method's composition never sends.
 */
export function sendsOnKey(target: Target, press: KeyPress): boolean {
  if (press.key !== "Enter" || press.isComposing) return false;
  switch (target.kind) {
    case "comment":
      return press.ctrlKey || press.metaKey;
    case "reply":
      return true;
  }
}
