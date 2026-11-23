import { describe, expect, it } from "vitest";
import {
  audienceLine,
  failureAction,
  failureMessage,
  fieldLabel,
  placeholder,
  sendsOnKey,
  sentAnnouncement,
  submitText,
  threadSummary,
  tooLongReason,
} from "./a11y.ts";
import { comment, mockupThread, tomas } from "./testThread.ts";
import type { Target } from "./thread.ts";

const page: Target = { kind: "comment" };
const toTomas: Target = { kind: "reply", to: tomas };
const plain = { ctrlKey: false, metaKey: false, shiftKey: false, isComposing: false };

describe("copy", () => {
  it("the summary counts comments and replies as the mockup writes them", () => {
    expect(threadSummary([])).toBe("None yet");
    expect(threadSummary([comment("c1", tomas, "x")])).toBe("1 comment");
    expect(threadSummary(mockupThread())).toBe("2 comments, 2 replies");
  });

  it("a reply field is named after the first name of the person it answers", () => {
    expect(fieldLabel(page)).toBe("Comment on the whole page");
    expect(fieldLabel(toTomas)).toBe("Reply to Tomás");
    expect(placeholder(toTomas, false)).toBe("Reply to Tomás");
  });

  it("the page field invites the first comment only when the thread is empty", () => {
    expect(placeholder(page, true)).toBe("Be the first to comment.");
    expect(placeholder(page, false)).toBe("Write a comment.");
  });

  it("the send button says Sending while a send is under way", () => {
    expect(submitText(page, false)).toBe("Comment");
    expect(submitText(toTomas, false)).toBe("Reply");
    expect(submitText(toTomas, true)).toBe("Sending…");
  });

  it("a send with no answer offers Retry and a refused send offers Copy text", () => {
    expect(failureMessage(toTomas, "no-answer", "Ana Okafor")).toBe(
      "Your reply was not sent because Flare did not answer. Your text is still in the field.",
    );
    expect(failureMessage(page, "refused", "Ana Okafor")).toBe(
      "You can no longer comment here, because you no longer have access to this page. Copy your text and ask Ana Okafor for access.",
    );
    expect(failureAction("no-answer")).toBe("Retry");
    expect(failureAction("refused")).toBe("Copy text");
  });

  it("the page field says who can read a comment, naming the owner by first name", () => {
    expect(audienceLine("Ana Okafor")).toBe(
      "Ana and everyone this page is shared with can read it.",
    );
  });

  it("a screen reader hears which kind of post was sent", () => {
    expect(sentAnnouncement(toTomas)).toBe("Reply sent");
    expect(sentAnnouncement(page)).toBe("Comment sent");
  });

  it("a text over the limit says by how many characters", () => {
    expect(tooLongReason(2012, 2000)).toBe(
      "Your text is 12 characters over the limit of 2,000 characters.",
    );
    expect(tooLongReason(2001, 2000)).toBe(
      "Your text is 1 character over the limit of 2,000 characters.",
    );
  });
});

describe("keyboard", () => {
  it("in the page field Enter adds a line and Ctrl or Command with Enter sends", () => {
    expect(sendsOnKey(page, { ...plain, key: "Enter" })).toBe(false);
    expect(sendsOnKey(page, { ...plain, key: "Enter", ctrlKey: true })).toBe(true);
    expect(sendsOnKey(page, { ...plain, key: "Enter", metaKey: true })).toBe(true);
  });

  it("in a reply field Enter sends", () => {
    expect(sendsOnKey(toTomas, { ...plain, key: "Enter" })).toBe(true);
    expect(sendsOnKey(toTomas, { ...plain, key: "a" })).toBe(false);
  });

  it("Enter that ends an input method's composition never sends", () => {
    expect(sendsOnKey(toTomas, { ...plain, key: "Enter", isComposing: true })).toBe(false);
    expect(sendsOnKey(page, { ...plain, key: "Enter", metaKey: true, isComposing: true })).toBe(
      false,
    );
  });
});
