import { describe, expect, it, vi } from "vitest";
import {
  blocker,
  type ComposerState,
  canRetry,
  createComposer,
  type Draft,
  type DraftKey,
  hasUnsentText,
  isDraftKey,
  overLimitLength,
  type SendResult,
  sendInactive,
  sendShown,
  showsAudience,
  showsSendHint,
} from "./composer.ts";
import { jonas, priya, reply } from "./testThread.ts";
import type { Reply, Target } from "./thread.ts";

const posted = reply("r3", "c2", priya, "Only for a cancelled sailing.");

function keys(): () => DraftKey {
  let next = 0;
  return () => {
    next += 1;
    const key = `key-${next}`;
    if (!isDraftKey(key)) throw new Error("unreachable");
    return key;
  };
}

// A send whose answer the test gives when it chooses.
function pendingSend() {
  const calls: { draft: Draft; answer: (result: SendResult<Reply>) => void; fail: () => void }[] =
    [];
  const send = vi.fn(
    (draft: Draft) =>
      new Promise<SendResult<Reply>>((resolve, reject) => {
        calls.push({ draft, answer: resolve, fail: () => reject(new Error("no answer")) });
      }),
  );
  return { send, calls };
}

function composer(send = pendingSend().send, maxLength = 2000) {
  return createComposer<Reply>({ send, newKey: keys(), maxLength });
}

function draftOf(state: ComposerState<Reply>): Draft | undefined {
  return "draft" in state ? state.draft : undefined;
}

describe("composer", () => {
  it("starts idle, and idle cannot send because the text is empty", () => {
    const c = composer();
    expect(c.getState()).toEqual({ status: "idle" });
    expect(blocker(c.getState(), 2000)).toBe("empty");
  });

  it("typing moves to composing with a key set before any send", () => {
    const c = composer();
    c.edit("Only for");
    expect(c.getState()).toEqual({
      status: "composing",
      draft: { key: "key-1", text: "Only for" },
    });
    expect(blocker(c.getState(), 2000)).toBeUndefined();
  });

  it("further typing keeps the draft's key", () => {
    const c = composer();
    c.edit("Only");
    c.edit("Only for");
    expect(draftOf(c.getState())?.key).toBe("key-1");
  });

  it("clearing the text returns to idle, and the next draft gets a new key", () => {
    const c = composer();
    c.edit("Only");
    c.edit("");
    expect(c.getState()).toEqual({ status: "idle" });
    c.edit("New");
    expect(draftOf(c.getState())?.key).toBe("key-2");
  });

  it("text of only spaces cannot be sent", () => {
    const c = composer();
    c.edit("   ");
    expect(blocker(c.getState(), 2000)).toBe("empty");
  });

  it("text over the limit cannot be sent, counting characters and not code units", () => {
    const c = composer(pendingSend().send, 3);
    c.edit("⛴⛴⛴");
    expect(blocker(c.getState(), 3)).toBeUndefined();
    c.edit("⛴⛴⛴⛴");
    expect(blocker(c.getState(), 3)).toBe("too-long");
  });

  it("a submit that is blocked does not call send", async () => {
    const { send } = pendingSend();
    const c = composer(send);
    await c.submit();
    c.edit("  ");
    await c.submit();
    expect(send).not.toHaveBeenCalled();
  });

  it("submitting locks the text and moves to sending", () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only for a cancelled sailing.");
    void c.submit();
    expect(c.getState().status).toBe("sending");
    expect(calls[0]?.draft).toEqual({ key: "key-1", text: "Only for a cancelled sailing." });
    c.edit("changed");
    expect(draftOf(c.getState())?.text).toBe("Only for a cancelled sailing.");
    expect(blocker(c.getState(), 2000)).toBe("sending");
  });

  it("a second submit while sending does not send twice", () => {
    const { send } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    void c.submit();
    void c.submit();
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("a send that is answered moves to sent with the posted reply and an empty field", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    const done = c.submit();
    calls[0]?.answer({ outcome: "sent", posted });
    await done;
    expect(c.getState()).toEqual({ status: "sent", posted });
    expect(blocker(c.getState(), 2000)).toBe("empty");
  });

  it("a send with no answer fails, keeps the text and offers retry", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    const done = c.submit();
    calls[0]?.fail();
    await done;
    expect(c.getState()).toEqual({
      status: "failed",
      draft: { key: "key-1", text: "Only" },
      failure: "no-answer",
    });
    expect(canRetry(c.getState())).toBe(true);
  });

  it("a retry sends the same key again, so the server can refuse a second post", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    const first = c.submit();
    calls[0]?.fail();
    await first;
    const second = c.submit();
    expect(c.getState().status).toBe("sending");
    calls[1]?.answer({ outcome: "sent", posted });
    await second;
    expect(calls.map((call) => call.draft.key)).toEqual(["key-1", "key-1"]);
    expect(c.getState().status).toBe("sent");
  });

  it("editing a failed draft keeps its key", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    const done = c.submit();
    calls[0]?.fail();
    await done;
    c.edit("Only for");
    expect(c.getState()).toEqual({
      status: "composing",
      draft: { key: "key-1", text: "Only for" },
    });
  });

  it("a refused send keeps the text and offers no retry", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    const done = c.submit();
    calls[0]?.answer({ outcome: "refused" });
    await done;
    expect(c.getState()).toEqual({
      status: "failed",
      draft: { key: "key-1", text: "Only" },
      failure: "refused",
    });
    expect(canRetry(c.getState())).toBe(false);
    expect(blocker(c.getState(), 2000)).toBe("refused");
    await c.submit();
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("editing a refused draft keeps it refused", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    const done = c.submit();
    calls[0]?.answer({ outcome: "refused" });
    await done;
    c.edit("Only for");
    expect(c.getState()).toMatchObject({ status: "failed", failure: "refused" });
    expect(draftOf(c.getState())?.text).toBe("Only for");
  });

  it("a full flare, an ended session and invalid text each keep the text and offer no retry", async () => {
    for (const reason of ["full", "signed-out", "invalid"] as const) {
      const { send, calls } = pendingSend();
      const c = composer(send);
      c.edit("Only");
      const done = c.submit();
      calls[0]?.answer({ outcome: "refused", reason });
      await done;
      expect(c.getState(), reason).toEqual({
        status: "failed",
        draft: { key: "key-1", text: "Only" },
        failure: reason,
      });
      expect(canRetry(c.getState()), reason).toBe(false);
      expect(sendShown(c.getState()), reason).toBe(false);
      expect(blocker(c.getState(), 2000), reason).toBe("refused");
      await c.submit();
      expect(send, reason).toHaveBeenCalledTimes(1);
    }
  });

  it("editing text the server called invalid lets it be sent again with the same key", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only\u0007");
    const done = c.submit();
    calls[0]?.answer({ outcome: "refused", reason: "invalid" });
    await done;
    c.edit("Only");
    expect(c.getState()).toEqual({ status: "composing", draft: { key: "key-1", text: "Only" } });
  });

  it("editing after a full flare or an ended session keeps the failure shown", async () => {
    for (const reason of ["full", "signed-out"] as const) {
      const { send, calls } = pendingSend();
      const c = composer(send);
      c.edit("Only");
      const done = c.submit();
      calls[0]?.answer({ outcome: "refused", reason });
      await done;
      c.edit("Only for");
      expect(c.getState(), reason).toMatchObject({ status: "failed", failure: reason });
    }
  });

  it("typing after a sent post starts a new draft with a new key", async () => {
    const { send, calls } = pendingSend();
    const c = composer(send);
    c.edit("Only");
    const done = c.submit();
    calls[0]?.answer({ outcome: "sent", posted });
    await done;
    c.edit("Next");
    expect(c.getState()).toEqual({ status: "composing", draft: { key: "key-2", text: "Next" } });
  });

  it("tells each listener of every change until it unsubscribes", () => {
    const c = composer();
    const listener = vi.fn();
    const unsubscribe = c.subscribe(listener);
    c.edit("a");
    c.edit("ab");
    unsubscribe();
    c.edit("abc");
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("an edit that changes nothing tells no listener", () => {
    const c = composer();
    const listener = vi.fn();
    c.subscribe(listener);
    c.edit("");
    expect(listener).not.toHaveBeenCalled();
  });
});

describe("what a field shows", () => {
  const page: Target = { kind: "comment" };
  const toJonas: Target = { kind: "reply", to: jonas };
  const draft = { key: "key-1", text: "Only" };
  const composing = (text: string): Extract<ComposerState<Reply>, { status: "composing" }> => {
    if (!isDraftKey(draft.key)) throw new Error("unreachable");
    return { status: "composing", draft: { key: draft.key, text } };
  };
  const failed = (failure: "no-answer" | "refused"): ComposerState<Reply> => {
    if (!isDraftKey(draft.key)) throw new Error("unreachable");
    return { status: "failed", draft: { key: draft.key, text: "Only" }, failure };
  };

  it("an empty page field's Comment is inactive, and an empty reply field's Reply stays active", () => {
    expect(sendInactive({ status: "idle" }, page, 2000)).toBe(true);
    expect(sendInactive({ status: "idle" }, toJonas, 2000)).toBe(false);
    expect(sendInactive(composing("  "), toJonas, 2000)).toBe(false);
  });

  it("Send is inactive while sending and over the limit, in either field", () => {
    for (const target of [page, toJonas]) {
      expect(sendInactive(composing("Only"), target, 2000)).toBe(false);
      expect(sendInactive(composing("Only"), target, 3)).toBe(true);
      expect(sendInactive({ status: "sending", draft: composing("x").draft }, target, 2000)).toBe(
        true,
      );
    }
  });

  it("Send is hidden after a refusal and shown in every other state", () => {
    expect(sendShown(failed("refused"))).toBe(false);
    expect(sendShown(failed("no-answer"))).toBe(true);
    expect(sendShown({ status: "idle" })).toBe(true);
  });

  it("text is unsent while composing, sending or failed", () => {
    expect(hasUnsentText({ status: "idle" })).toBe(false);
    expect(hasUnsentText({ status: "sent", posted })).toBe(false);
    expect(hasUnsentText(composing("Only"))).toBe(true);
    expect(hasUnsentText({ status: "sending", draft: composing("x").draft })).toBe(true);
    expect(hasUnsentText(failed("refused"))).toBe(true);
  });

  it("the over-limit length counts characters, and is absent within the limit", () => {
    expect(overLimitLength(composing("⛴⛴⛴⛴"), 3)).toBe(4);
    expect(overLimitLength(composing("⛴⛴⛴"), 3)).toBeUndefined();
    expect(overLimitLength({ status: "idle" }, 3)).toBeUndefined();
  });

  it("the page field says who can read it while idle or composing, and gives the send hint only while composing", () => {
    expect(showsAudience({ status: "idle" }, page)).toBe(true);
    expect(showsAudience(composing("Only"), page)).toBe(true);
    expect(showsAudience({ status: "sending", draft: composing("x").draft }, page)).toBe(false);
    expect(showsAudience({ status: "idle" }, toJonas)).toBe(false);
    expect(showsSendHint(composing("Only"), page)).toBe(true);
    expect(showsSendHint({ status: "idle" }, page)).toBe(false);
    expect(showsSendHint(composing("Only"), toJonas)).toBe(false);
  });
});
