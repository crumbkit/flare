import { describe, expect, it, vi } from "vitest";
import {
  blocker,
  type ComposerState,
  canRetry,
  createComposer,
  type Draft,
  type DraftKey,
  isDraftKey,
  type SendResult,
} from "./composer.ts";
import { priya, reply } from "./testThread.ts";
import type { Reply } from "./thread.ts";

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
