import type { Draft, SendResult } from "@flare/headless/comments/composer";
import type { Comment, Reply, ThreadLoad } from "@flare/headless/comments/thread";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { commentId, mockupThread, postedReply, Section } from "./testSection.tsx";

const never = <P,>() => vi.fn((_: Draft) => new Promise<SendResult<P>>(() => undefined));

function section(load: ThreadLoad, onRetry = vi.fn()) {
  return render(
    <Section
      load={load}
      onRetry={onRetry}
      sendReply={never<Reply>()}
      sendComment={never<Comment>()}
    />,
  );
}

describe("the comments section", () => {
  it("while loading is busy and says Loading comments in a status", () => {
    const { container } = section({ state: "loading" });
    expect(screen.getByRole("heading", { name: "Comments" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Loading comments");
    expect(container.querySelector("section")).toHaveAttribute("aria-busy", "true");
  });

  it("when loading failed shows an alert whose Retry loads again", async () => {
    const onRetry = vi.fn();
    const { container } = section({ state: "failed" }, onRetry);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "The comments did not load because Flare did not answer. The page above is unaffected.",
    );
    expect(container.querySelector("section")).not.toHaveAttribute("aria-busy");
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("with no comments says None yet and invites the first comment", () => {
    section({ state: "ready", thread: [] });
    expect(screen.getByText("None yet")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Comment on the whole page" })).toHaveAttribute(
      "placeholder",
      "Be the first to comment.",
    );
  });

  it("shows every comment with its replies and counts them", () => {
    section({ state: "ready", thread: mockupThread() });
    expect(screen.getByText("2 comments, 1 reply")).toBeInTheDocument();
    expect(screen.getAllByRole("article")).toHaveLength(2);
    expect(screen.getByText("Yes. All three do.")).toBeInTheDocument();
  });

  it("puts a sent reply at the end of its thread and counts it", async () => {
    const posted = postedReply(
      commentId("c2"),
      { name: "Priya Nair" },
      "Only for a cancelled one.",
    );
    let answer: (r: SendResult<Reply>) => void = () => undefined;
    const sendReply = vi.fn(
      (_: Draft) =>
        new Promise<SendResult<Reply>>((resolve) => {
          answer = resolve;
        }),
    );
    render(
      <Section
        load={{ state: "ready", thread: mockupThread() }}
        onRetry={vi.fn()}
        sendReply={sendReply}
        sendComment={never<Comment>()}
      />,
    );
    await userEvent.type(
      screen.getByRole("textbox", { name: "Reply to Jonas" }),
      "Only for a cancelled one.{Enter}",
    );
    answer({ outcome: "sent", posted });
    const [, jonasThread] = screen.getAllByRole("article");
    if (!jonasThread) throw new Error("unreachable");
    expect(await within(jonasThread).findByText("Only for a cancelled one.")).toBeInTheDocument();
    expect(screen.getByText("2 comments, 2 replies")).toBeInTheDocument();
  });

  it("tabs through each reply field and its button in reading order, then the page field and Comment", async () => {
    section({ state: "ready", thread: mockupThread() });
    const order = [
      screen.getByRole("textbox", { name: "Reply to Tomás" }),
      screen.getAllByRole("button", { name: "Reply" })[0],
      screen.getByRole("textbox", { name: "Reply to Jonas" }),
      screen.getAllByRole("button", { name: "Reply" })[1],
      screen.getByRole("textbox", { name: "Comment on the whole page" }),
      screen.getByRole("button", { name: "Comment" }),
    ];
    for (const element of order) {
      await userEvent.tab();
      expect(element).toHaveFocus();
    }
  });
});
