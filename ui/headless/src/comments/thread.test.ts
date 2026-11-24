import { describe, expect, it } from "vitest";
import { comment, jonas, mockupThread, priya, reply } from "./testThread.ts";
import { isCommentId, isReplyId, replyTarget, withPosted } from "./thread.ts";

describe("thread", () => {
  it("an empty string is neither a comment id nor a reply id", () => {
    expect(isCommentId("")).toBe(false);
    expect(isReplyId("")).toBe(false);
    expect(isCommentId("c1")).toBe(true);
  });

  it("a posted reply joins the end of its parent's replies", () => {
    const thread = withPosted(
      mockupThread(),
      reply("r3", "c2", priya, "Only for a cancelled one."),
    );
    expect(thread[1]?.replies.map((r) => r.id)).toEqual(["r3"]);
    expect(thread[0]?.replies.map((r) => r.id)).toEqual(["r1", "r2"]);
  });

  it("a posted comment joins the end of the thread", () => {
    const thread = withPosted(mockupThread(), comment("c3", jonas, "Another point."));
    expect(thread.map((c) => c.id)).toEqual(["c1", "c2", "c3"]);
  });

  it("a post the thread already holds is not added twice", () => {
    const once = withPosted(mockupThread(), reply("r3", "c2", priya, "Only for a cancelled one."));
    const twice = withPosted(once, reply("r3", "c2", priya, "Only for a cancelled one."));
    expect(twice[1]?.replies).toHaveLength(1);
    expect(withPosted(once, comment("c1", jonas, "x"))).toHaveLength(2);
  });

  it("a reply answers its comment's author, and a deleted comment's reply answers no one", () => {
    const [first] = mockupThread();
    if (first === undefined) throw new Error("expected the mockup's first comment");
    expect(replyTarget(first)).toEqual({
      kind: "reply",
      to: first.deleted ? undefined : first.author,
    });
    const deleted = {
      kind: "comment",
      id: first.id,
      deleted: true,
      replies: first.replies,
    } as const;
    expect(replyTarget(deleted)).toEqual({ kind: "reply", to: undefined });
  });

  it("a reply whose parent is not in the thread leaves the thread as it was", () => {
    const thread = mockupThread();
    expect(withPosted(thread, reply("r9", "gone", priya, "Hello."))).toEqual(thread);
  });
});
