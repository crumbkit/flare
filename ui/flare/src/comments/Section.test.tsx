// The comments section in each state that artboards 01 to 07 of the comment
// parts mockup (LRJsZhYqKPHcug6pQKw8dA, version 1795398322-8ae4) draw, with
// flare-server's comment routes stood in for by testServer.ts.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { CommentsSection } from "./Section.tsx";
import { type Answer, mockupComments, owner, testServer, type WireComment } from "./testServer.ts";

const NOW = "2026-11-23T10:00:00Z";
const audience = "ana.okafor@example.com and everyone this page is shared with can read it.";
const text = "Could the notice say when the next forecast arrives?";

function show(comments: WireComment[] = mockupComments(), reads: Answer[] = []) {
  const server = testServer(comments, NOW);
  server.next.read.push(...reads);
  vi.stubGlobal("fetch", server.fetch);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <CommentsSection flare="f1" owner={owner} now={() => Date.parse(NOW)} timeZone="UTC" />
    </QueryClientProvider>,
  );
  return server;
}

const pageField = () => screen.getByRole("textbox", { name: "Comment on the whole page" });
const commentButton = () => screen.getByRole("button", { name: "Comment" });
const replyField = (to: string) => screen.getByRole("textbox", { name: `Reply to ${to}` });

describe("artboard 01: loading, empty and failed to load", () => {
  it("says it is loading the comments while it waits", async () => {
    const server = show(mockupComments(), ["hold"]);
    expect(screen.getByRole("heading", { level: 2, name: "Comments" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Loading comments");
    server.release();
    expect(await screen.findByText("2 comments, 2 replies")).toBeInTheDocument();
  });

  it("invites the first comment when there are none, with Comment inactive", async () => {
    show([]);
    expect(await screen.findByText("None yet")).toBeInTheDocument();
    expect(pageField()).toHaveAttribute("placeholder", "Be the first to comment.");
    expect(commentButton()).toHaveAttribute("aria-disabled", "true");
    expect(screen.queryByText(audience)).not.toBeInTheDocument();
  });

  it("offers Retry when Flare did not answer, and Retry loads the comments", async () => {
    show(mockupComments(), [503]);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "The comments did not load because Flare did not answer. The page above is unaffected.",
    );
    await userEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("2 comments, 2 replies")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("says the reader lost access, with no Retry, when the comments are refused", async () => {
    show(mockupComments(), [404]);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "The comments did not load because you no longer have access to this page. Ask ana.okafor@example.com for access.",
    );
    expect(within(alert).queryByRole("button")).not.toBeInTheDocument();
  });
});

describe("artboard 02: idle", () => {
  it("lists every comment with its replies, each author by initials, identifier and time", async () => {
    show();
    expect(await screen.findByText("2 comments, 2 replies")).toBeInTheDocument();
    const [first, second] = screen.getAllByRole("article");
    if (!first || !second) throw new Error("expected two comments");
    expect(within(first).getByText("tomas.rivera@example.com")).toBeInTheDocument();
    expect(within(first).getByText("TR")).toBeInTheDocument();
    expect(within(first).getByText("21 Nov, 16:02")).toHaveAttribute(
      "datetime",
      "2026-11-21T16:02:00Z",
    );
    expect(within(first).getByText("Yes. All three do.")).toBeInTheDocument();
    expect(within(first).getByText("AO")).toBeInTheDocument();
    expect(within(second).getByText("Do we pay for a night's stay?")).toBeInTheDocument();
    expect(replyField("tomas.rivera@example.com")).toHaveAttribute(
      "placeholder",
      "Reply to tomas.rivera@example.com",
    );
    expect(within(second).getByRole("button", { name: "Reply" })).toBeInTheDocument();
    expect(pageField()).toHaveAttribute("placeholder", "Write a comment.");
    expect(commentButton()).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByText(audience)).toBeInTheDocument();
  });
});

describe("artboard 03: composing", () => {
  it("activates Comment, shows the send hint, adds a line on Enter and sends on Ctrl with Enter", async () => {
    const server = show();
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Comment on the whole page" }),
      "One{Enter}two",
    );
    expect(pageField()).toHaveValue("One\ntwo");
    expect(commentButton()).not.toHaveAttribute("aria-disabled");
    expect(screen.getByText("Ctrl or ⌘ + Enter to send")).toBeInTheDocument();
    expect(screen.getByText(audience)).toBeInTheDocument();
    expect(server.posts).toEqual([]);
    await userEvent.keyboard("{Control>}{Enter}{/Control}");
    await waitFor(() => expect(server.posts.map((p) => p.body)).toEqual(["One\ntwo"]));
  });

  it("asks before the page is left with unsent text", async () => {
    show();
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Comment on the whole page" }),
      text,
    );
    const leave = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(leave);
    expect(leave.defaultPrevented).toBe(true);
  });
});

describe("artboard 04: sending", () => {
  it("locks the text and says Sending until Flare answers, in the page field and a reply field", async () => {
    const server = show();
    server.next.write.push("hold", "hold");
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Comment on the whole page" }),
      text,
    );
    await userEvent.click(commentButton());
    expect(pageField()).toHaveAttribute("readonly");
    const sending = screen.getByRole("button", { name: "Sending…" });
    expect(sending).toHaveAttribute("aria-busy", "true");
    expect(sending).toHaveAttribute("aria-disabled", "true");
    await userEvent.type(
      replyField("jonas.weber@example.com"),
      "Only for a cancelled sailing.{Enter}",
    );
    expect(replyField("jonas.weber@example.com")).toHaveAttribute("readonly");
    expect(screen.getAllByRole("button", { name: "Sending…" })).toHaveLength(2);
    server.release();
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Sending…" })).not.toBeInTheDocument(),
    );
  });
});

describe("artboard 05: failed, with retry", () => {
  it("keeps the text when Flare did not answer, and Retry sends it once with the same key", async () => {
    const server = show();
    server.next.write.push("drop");
    const field = await screen.findByRole("textbox", { name: "Reply to jonas.weber@example.com" });
    await userEvent.type(field, "Only for a cancelled sailing.{Enter}");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "Your reply was not sent because Flare did not answer. Your text is still in the field.",
    );
    expect(field).toHaveValue("Only for a cancelled sailing.");
    await userEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Only for a cancelled sailing.")).toBeInTheDocument();
    const [first, second] = server.posts;
    expect(server.posts).toHaveLength(2);
    expect(second?.key).toBe(first?.key);
  });

  it("keeps the text and offers Copy with no Retry when the send is refused", async () => {
    const server = show();
    server.next.write.push(404);
    const user = userEvent.setup();
    await user.type(
      await screen.findByRole("textbox", { name: "Comment on the whole page" }),
      text,
    );
    await user.click(commentButton());
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "You can no longer comment here, because you no longer have access to this page. Copy your text and ask ana.okafor@example.com for access.",
    );
    expect(pageField()).toHaveValue(text);
    expect(pageField()).toHaveAttribute("aria-describedby", alert.id);
    expect(screen.queryByRole("button", { name: "Comment" })).not.toBeInTheDocument();
    expect(within(alert).queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    await user.click(within(alert).getByRole("button", { name: "Copy text" }));
    expect(await navigator.clipboard.readText()).toBe(text);
  });

  it("keeps the text and offers Copy when the flare holds as many comments as it may", async () => {
    const server = show();
    server.next.write.push(409);
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Comment on the whole page" }),
      text,
    );
    await userEvent.click(commentButton());
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByRole("button", { name: "Copy text" })).toBeInTheDocument();
    expect(pageField()).toHaveValue(text);
  });
});

describe("artboard 06: replied", () => {
  it("adds the reply to the end of its thread as just now, empties the field, keeps focus and says Reply sent", async () => {
    show();
    const field = await screen.findByRole("textbox", { name: "Reply to jonas.weber@example.com" });
    await userEvent.type(field, "Only for a cancelled sailing.{Enter}");
    expect(await screen.findByText("Only for a cancelled sailing.")).toBeInTheDocument();
    const second = screen.getAllByRole("article")[1];
    if (!second) throw new Error("expected a second comment");
    expect(within(second).getByText("just now")).toBeInTheDocument();
    expect(within(second).getByText("priya.nair@example.com")).toBeInTheDocument();
    expect(field).toHaveValue("");
    expect(field).toHaveFocus();
    expect(within(second).getByRole("status")).toHaveTextContent("Reply sent");
    expect(screen.getByText("2 comments, 3 replies")).toBeInTheDocument();
  });

  it("adds a page comment after the last one and says Comment sent", async () => {
    show();
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Comment on the whole page" }),
      text,
    );
    await userEvent.click(commentButton());
    await waitFor(() => expect(screen.getAllByRole("article")).toHaveLength(3));
    const third = screen.getAllByRole("article")[2];
    expect(third).toHaveTextContent(text);
    expect(pageField()).toHaveValue("");
    expect(pageField()).toHaveFocus();
    expect(screen.getByText("Comment sent")).toBeInTheDocument();
    expect(screen.getByText("3 comments, 2 replies")).toBeInTheDocument();
  });
});

describe("artboard 07: keyboard focus order", () => {
  it("visits each reply field and its button in reading order, then the page field and Comment", async () => {
    show();
    await screen.findByText("2 comments, 2 replies");
    const order = [
      replyField("tomas.rivera@example.com"),
      screen.getAllByRole("button", { name: "Reply" })[0],
      replyField("jonas.weber@example.com"),
      screen.getAllByRole("button", { name: "Reply" })[1],
      pageField(),
      commentButton(),
    ];
    for (const element of order) {
      await userEvent.tab();
      expect(element).toHaveFocus();
    }
  });
});
