import { describe, expect, it } from "vitest";
import {
  failureMessage,
  fieldLabel,
  placeholder,
  sendsOnKey,
  sentAnnouncement,
  submitText,
} from "./a11y.ts";
import { blocker, type ComposerState, sendInactive, showsAudience } from "./composer.ts";
import {
  cancelText,
  closesPassage,
  commentedOn,
  marginLabel,
  movedNote,
  outdatedNote,
  quoteText,
} from "./passage.ts";
import type { Target } from "./thread.ts";

const passage: Target = { kind: "passage" };
const plain = { ctrlKey: false, metaKey: false, isComposing: false };

describe("the card on highlighted text, as artboard 08 draws it", () => {
  it("names its field, its buttons and its quote with the mockup's words", () => {
    expect(fieldLabel(passage)).toBe("Your comment on this text");
    expect(placeholder(passage, false)).toBe("");
    expect(submitText(passage, false)).toBe("Comment");
    expect(submitText(passage, true)).toBe("Sending…");
    expect(cancelText).toBe("Cancel");
    expect(quoteText("more than 5 knots above the limit cancels the sailing at once")).toBe(
      '"more than 5 knots above the limit cancels the sailing at once"',
    );
  });

  it("opens with Comment inactive and shows no audience line", () => {
    const idle: ComposerState<unknown> = { status: "idle" };
    expect(sendInactive(idle, passage, 8000)).toBe(true);
    expect(blocker(idle, 8000)).toBe("empty");
    expect(showsAudience(idle, passage)).toBe(false);
  });

  it("adds a line on Enter and sends on Ctrl or Command with Enter, as the page field does", () => {
    expect(sendsOnKey(passage, { ...plain, key: "Enter" })).toBe(false);
    expect(sendsOnKey(passage, { ...plain, key: "Enter", ctrlKey: true })).toBe(true);
    expect(sendsOnKey(passage, { ...plain, key: "Enter", metaKey: true })).toBe(true);
  });

  it("says comment, not passage, in its failures and its announcement", () => {
    expect(failureMessage(passage, "no-answer", "Ana Okafor", "x")).toBe(
      "Your comment was not sent because Flare did not answer. Your text is still in the field.",
    );
    expect(sentAnnouncement(passage)).toBe("Comment sent");
  });

  it("closes on Escape and Cancel, and on a cleared selection only while empty and unfocused", () => {
    expect(closesPassage("escape", "half a thought", true)).toBe(true);
    expect(closesPassage("cancel", "half a thought", true)).toBe(true);
    expect(closesPassage("selection-cleared", "", false)).toBe(true);
    expect(closesPassage("selection-cleared", "", true)).toBe(false);
    expect(closesPassage("selection-cleared", "half a thought", false)).toBe(false);
  });

  it("labels the margin while composing and while it only holds comments", () => {
    expect(marginLabel({ composing: true, narrow: false })).toBe("Comment on this passage");
    expect(marginLabel({ composing: true, narrow: true })).toBe("Comment on this passage");
    expect(marginLabel({ composing: false, narrow: false })).toBe("Comments on this page's text");
    expect(marginLabel({ composing: false, narrow: true })).toBe("Comments on this passage");
  });
});

describe("the card on a later version, as artboards 10 and 11 draw it", () => {
  it("says a moved comment was made on its version and the text has moved", () => {
    expect(movedNote(2)).toBe("Made on version 2. The text has moved since then.");
  });

  it("says an outdated comment's text changed on the viewed version, and which version it was made on", () => {
    expect(outdatedNote(3)).toBe("The quoted text changed on version 3.");
    expect(commentedOn(2)).toBe("Commented on version 2");
  });
});
