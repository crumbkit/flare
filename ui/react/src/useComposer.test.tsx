import type { Draft, SendResult } from "@flare/headless/comments/composer";
import type { Comment, Reply } from "@flare/headless/comments/thread";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  commentId,
  jonas,
  mockupThread,
  PageField,
  postedReply,
  ReplyField,
} from "./testSection.tsx";

// A send whose answer the test gives when it chooses.
function pendingSend<P>() {
  const calls: { draft: Draft; answer: (r: SendResult<P>) => void; fail: () => void }[] = [];
  const send = vi.fn(
    (draft: Draft) =>
      new Promise<SendResult<P>>((resolve, reject) => {
        calls.push({ draft, answer: resolve, fail: () => reject(new Error("no answer")) });
      }),
  );
  return { send, calls };
}

const jonasComment = (): Comment => {
  const [, second] = mockupThread();
  if (!second) throw new Error("unreachable");
  return second;
};

const text = "Could the notice say when the next forecast arrives?";

describe("the page comment field", () => {
  it("is labelled, invites the first comment and keeps Comment inactive while empty", async () => {
    const { send } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={true} />);
    const field = screen.getByRole("textbox", { name: "Comment on the whole page" });
    expect(field).toHaveAttribute("placeholder", "Be the first to comment.");
    const button = screen.getByRole("button", { name: "Comment" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAttribute("type", "submit");
    await userEvent.click(button);
    expect(send).not.toHaveBeenCalled();
  });

  it("activates Comment and shows the send hint once it holds text", async () => {
    const { send } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={false} />);
    const field = screen.getByRole("textbox", { name: "Comment on the whole page" });
    expect(field).toHaveAttribute("placeholder", "Write a comment.");
    await userEvent.type(field, "Could");
    expect(screen.getByRole("button", { name: "Comment" })).not.toHaveAttribute("aria-disabled");
    expect(screen.getByText("Ctrl or ⌘ + Enter to send")).toBeInTheDocument();
  });

  it("adds a line on Enter and sends on Ctrl or Command with Enter", async () => {
    const { send, calls } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={false} />);
    const field = screen.getByRole("textbox", { name: "Comment on the whole page" });
    await userEvent.type(field, "One{Enter}two");
    expect(field).toHaveValue("One\ntwo");
    expect(send).not.toHaveBeenCalled();
    await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
    expect(calls[0]?.draft.text).toBe("One\ntwo");
  });

  it("sends on Ctrl with Enter", async () => {
    const { send } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={false} />);
    await userEvent.type(screen.getByRole("textbox"), "One");
    await userEvent.keyboard("{Control>}{Enter}{/Control}");
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("locks the text and says Sending until Flare answers, so a second press cannot post twice", async () => {
    const { send } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={false} />);
    const field = screen.getByRole("textbox");
    await userEvent.type(field, text);
    await userEvent.click(screen.getByRole("button", { name: "Comment" }));
    const button = screen.getByRole("button", { name: "Sending…" });
    expect(button).toHaveAttribute("aria-busy", "true");
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(field).toHaveAttribute("readonly");
    await userEvent.click(button);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("on a refusal keeps the text, offers Copy text and no Retry, and names the owner", async () => {
    const user = userEvent.setup();
    const { send, calls } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={false} />);
    const field = screen.getByRole("textbox");
    await user.type(field, text);
    await user.click(screen.getByRole("button", { name: "Comment" }));
    calls[0]?.answer({ outcome: "refused" });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "You can no longer comment here, because you no longer have access to this page. Copy your text and ask Ana Okafor for access.",
    );
    expect(field).toHaveValue(text);
    expect(field).toHaveAttribute("aria-describedby", alert.id);
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Comment" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Copy text" }));
    expect(await navigator.clipboard.readText()).toBe(text);
  });

  it("disables Comment with a reason when the text is over the limit", async () => {
    const { send } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={false} maxLength={5} />);
    const field = screen.getByRole("textbox");
    await userEvent.type(field, "Sixsix");
    const button = screen.getByRole("button", { name: "Comment" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(field).toHaveAccessibleDescription(
      "Your text is 1 character over the limit of 5 characters.",
    );
    await userEvent.click(button);
    expect(send).not.toHaveBeenCalled();
  });

  it("asks before the page closes while it holds unsent text, and not while empty", async () => {
    const { send } = pendingSend<Comment>();
    render(<PageField send={send} threadIsEmpty={false} />);
    const leave = () => {
      const event = new Event("beforeunload", { cancelable: true });
      window.dispatchEvent(event);
      return event.defaultPrevented;
    };
    expect(leave()).toBe(false);
    await userEvent.type(screen.getByRole("textbox"), "Draft");
    expect(leave()).toBe(true);
    await userEvent.clear(screen.getByRole("textbox"));
    expect(leave()).toBe(false);
  });
});

describe("a reply field", () => {
  it("is named after the person it answers and sends on Enter", async () => {
    const { send, calls } = pendingSend<Reply>();
    render(<ReplyField parent={jonasComment()} send={send} onSent={vi.fn()} />);
    const field = screen.getByRole("textbox", { name: "Reply to Jonas" });
    expect(field).toHaveAttribute("placeholder", "Reply to Jonas");
    const button = screen.getByRole("button", { name: "Reply" });
    expect(button).toHaveAttribute("type", "button");
    // The mockup leaves an empty reply field's button active.
    expect(button).not.toHaveAttribute("aria-disabled");
    await userEvent.type(field, "Only for a cancelled sailing.{Enter}");
    expect(calls[0]?.draft.text).toBe("Only for a cancelled sailing.");
  });

  it("on no answer keeps the text and offers Retry, which resends with the same key", async () => {
    const { send, calls } = pendingSend<Reply>();
    render(<ReplyField parent={jonasComment()} send={send} onSent={vi.fn()} />);
    const field = screen.getByRole("textbox", { name: "Reply to Jonas" });
    await userEvent.type(field, "Only for a cancelled sailing.{Enter}");
    calls[0]?.fail();
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "Your reply was not sent because Flare did not answer. Your text is still in the field.",
    );
    expect(field).toHaveValue("Only for a cancelled sailing.");
    expect(field).toHaveAttribute("aria-describedby", alert.id);
    expect(screen.getByRole("button", { name: "Reply" })).not.toHaveAttribute("aria-disabled");
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(calls.map((call) => call.draft.key)).toEqual(["key-1", "key-1"]);
  });

  it("when sent empties the field, keeps its focus and announces Reply sent", async () => {
    const { send, calls } = pendingSend<Reply>();
    const onSent = vi.fn();
    render(<ReplyField parent={jonasComment()} send={send} onSent={onSent} />);
    const field = screen.getByRole("textbox", { name: "Reply to Jonas" });
    await userEvent.type(field, "Only for a cancelled sailing.");
    await userEvent.click(screen.getByRole("button", { name: "Reply" }));
    const posted = postedReply(commentId("c2"), jonas, "Only for a cancelled sailing.");
    calls[0]?.answer({ outcome: "sent", posted });
    expect(await screen.findByRole("status")).toHaveTextContent("Reply sent");
    expect(field).toHaveValue("");
    await waitFor(() => expect(field).toHaveFocus());
    expect(onSent).toHaveBeenCalledWith(posted);
  });
});
