// The words and rules of the card on highlighted text, as artboards 08 to
// 13 of the comment parts mockup (UBmgioBRkLLV9B7YMxdhEU, version
// 1795819564-9bd4) draw it: the card that composes a comment on the text a
// reader selected, and the card of a comment on a later version, whose text
// has moved or changed.

/** The card's button that closes it without sending. */
export const cancelText = "Cancel";

/** The quote as the card shows it, in straight double quotes. */
export function quoteText(quote: string): string {
  return `"${quote}"`;
}

/** The line under a moved comment's quote (artboard 10). */
export function movedNote(version: number): string {
  return `Made on version ${version}. The text has moved since then.`;
}

/** The line at the top of an outdated comment's card, naming the viewed version (artboard 11). */
export function outdatedNote(viewed: number): string {
  return `The quoted text changed on version ${viewed}.`;
}

/** The label above an outdated comment's quote (artboard 11). */
export function commentedOn(version: number): string {
  return `Commented on version ${version}`;
}

/**
 * The accessible name of the cards beside the page text: the margin of a
 * wide page, or the cards under one paragraph on a phone.
 */
export function marginLabel(where: { readonly composing: boolean; readonly narrow: boolean }) {
  if (where.composing) return "Comment on this passage";
  return where.narrow ? "Comments on this passage" : "Comments on this page's text";
}

/** What may close the card while it composes. */
export type CloseReason = "escape" | "cancel" | "selection-cleared";

/**
 * Whether the composing card closes. Escape in its field and Cancel close
 * it. A cleared selection closes it only while it is empty and holds no
 * focus, so a reader can click into the field, and text they typed stays.
 */
export function closesPassage(why: CloseReason, text: string, focused: boolean): boolean {
  if (why !== "selection-cleared") return true;
  return text === "" && !focused;
}
