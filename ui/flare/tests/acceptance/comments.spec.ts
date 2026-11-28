import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

// A signed-in reader comments on a shared page and replies to the comment,
// in a real browser against flare-server with memory stores, as
// crates/flare-server/tests/server/reading.rs starts it. That test gives the
// server's origin, the reader's session cookie and the share's id.
function setting(name: string): string {
  const value = process.env[name];
  if (value === undefined || value === "") throw new Error(`${name} is not set`);
  return value;
}

const origin = setting("FLARE_JOURNEY_ORIGIN");
const cookie = setting("FLARE_JOURNEY_COOKIE");
const share = setting("FLARE_JOURNEY_SHARE");

/** Collects Content-Security-Policy violations and logged errors. */
async function watch(page: Page): Promise<{ violations: string[]; errors: string[] }> {
  const violations: string[] = [];
  const errors: string[] = [];
  await page.exposeFunction("reportViolation", (text: string) => violations.push(text));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (event) => {
      const report = Reflect.get(window, "reportViolation");
      if (typeof report === "function") {
        report(`${event.effectiveDirective} blocked ${event.blockedURI}`);
      }
    });
  });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  return { violations, errors };
}

async function axe(page: Page): Promise<string[]> {
  const report = await new AxeBuilder({ page }).analyze();
  return report.violations.flatMap((violation) =>
    violation.nodes.map(
      (node) => `${violation.id}: ${node.target.join(" ")}: ${node.failureSummary ?? ""}`,
    ),
  );
}

test("a signed-in reader comments on a shared page and replies, and both stay after a reload", async ({
  page,
  context,
}, info) => {
  // Chromium refuses a `__Host-` cookie set through the DevTools protocol
  // for an http origin, so every request the page makes carries the
  // session cookie as a header instead.
  await context.setExtraHTTPHeaders({ Cookie: cookie });
  const { violations, errors } = await watch(page);
  const comment = `Could the notice say when the next forecast arrives? (${info.project.name})`;
  const reply = `Only for a cancelled sailing. (${info.project.name})`;

  await page.goto(`${origin}/s/${share}`);
  const section = page.getByRole("region", { name: "Comments" });
  const field = section.getByRole("textbox", { name: "Comment on the whole page" });
  await expect(field).toBeVisible();
  await expect(
    page.getByRole("link", { name: /^(No comments|\d+ comments?(, \d+ repl(y|ies))?)$/ }),
  ).toHaveAttribute("href", "#comments");
  expect(await axe(page)).toEqual([]);

  await field.fill(comment);
  await expect(section.getByText("Ctrl or ⌘ + Enter to send")).toBeVisible();
  await section.getByRole("button", { name: "Comment" }).click();
  const card = section.getByRole("article").filter({ hasText: comment });
  await expect(card).toBeVisible();
  await expect(section.getByRole("status").filter({ hasText: "Comment sent" })).toHaveCount(1);
  await expect(field).toHaveValue("");
  await expect(field).toBeFocused();

  const replyField = card.getByRole("textbox", { name: "Reply to bob@example.com" });
  await replyField.fill(reply);
  await replyField.press("Enter");
  await expect(card.getByText(reply)).toBeVisible();
  await expect(card.getByRole("status")).toHaveText("Reply sent");
  await expect(replyField).toBeFocused();

  await page.reload();
  const kept = section.getByRole("article").filter({ hasText: comment });
  await expect(kept).toBeVisible();
  await expect(kept.getByText(reply)).toBeVisible();
  await expect(kept.getByText("BO", { exact: true })).toHaveCount(2);

  // Tab goes from a reply field to its Reply button, and from the page
  // field to Comment (artboard 07).
  await kept.getByRole("textbox", { name: "Reply to bob@example.com" }).focus();
  await page.keyboard.press("Tab");
  await expect(kept.getByRole("button", { name: "Reply" })).toBeFocused();
  await field.focus();
  await page.keyboard.press("Tab");
  await expect(section.getByRole("button", { name: "Comment" })).toBeFocused();

  // Once its author deletes the comment, its card keeps its place above the
  // reply and says so in muted italic type, with no author or time.
  // The request carries the headers flare-server requires of a cookie write
  // from one of its own pages.
  const own = { Origin: origin, "Sec-Fetch-Site": "same-origin" };
  const listed = await page.request.get(
    `${origin}/api/comments?flare=${encodeURIComponent(share)}`,
    { headers: own },
  );
  expect(listed.status()).toBe(200);
  const thread: { comments: { id: string; body?: string | null }[] } = await listed.json();
  const posted = thread.comments.find((each) => each.body === comment);
  if (posted === undefined) throw new Error("the comment is not in the thread");
  const deleted = await page.request.delete(
    `${origin}/api/comments/${encodeURIComponent(posted.id)}`,
    { headers: { ...own, "Content-Type": "application/json" } },
  );
  const status = deleted.status();
  expect(status).toBe(200);
  await page.reload();
  const orphaned = section.getByRole("article").filter({ hasText: reply });
  const gone = orphaned.getByText("Its author deleted this comment.");
  await expect(gone).toBeVisible();
  await expect(gone).toHaveCSS("font-style", "italic");
  const muted = await section
    .locator(".section-head .muted")
    .evaluate((element) => getComputedStyle(element).color);
  await expect(gone).toHaveCSS("color", muted);
  await expect(orphaned.getByText("BO", { exact: true })).toHaveCount(1);
  await expect(orphaned.getByText(comment)).toHaveCount(0);
  await expect(orphaned.getByRole("textbox", { name: "Reply", exact: true })).toHaveAttribute(
    "placeholder",
    "Write a reply.",
  );

  expect(await axe(page)).toEqual([]);
  expect(violations).toEqual([]);
  expect(errors).toEqual([]);
  await page.screenshot({
    path: info.outputPath(`comments-${info.project.name}.png`),
    fullPage: true,
  });
});

/** Selects the first run of `text` in the article, as a reader's drag would. */
async function selectInArticle(page: Page, text: string): Promise<void> {
  await page.evaluate((wanted) => {
    const article = document.querySelector("main article.doc");
    if (article === null) throw new Error("the page has no article");
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
      const at = node.textContent?.indexOf(wanted) ?? -1;
      if (at < 0) continue;
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + wanted.length);
      const selection = document.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      return;
    }
    throw new Error(`the article has no "${wanted}"`);
  }, text);
}

test("a reader comments on selected text, sees the card beside it or under its paragraph, and it stays after a reload", async ({
  page,
  context,
}, info) => {
  await context.setExtraHTTPHeaders({ Cookie: cookie });
  const { violations, errors } = await watch(page);
  const quote = "more than 5 knots above the limit cancels the sailing at once";
  const comment = `Does the 5 knot margin apply to the Curlew too? (${info.project.name})`;
  const phone = info.project.name === "phone";

  await page.goto(`${origin}/s/${share}`);
  const article = page.locator("main article.doc");
  await expect(page.getByRole("region", { name: "Comments" })).toBeVisible();
  await selectInArticle(page, quote);

  // Artboards 08 and 09: the card opens with the quote and a labelled field,
  // and leaves focus in the page.
  // The other projects run at once on the same share, so the margin may
  // already hold their cards on this text; the composing card is the form.
  const margin = page.getByRole("complementary", { name: "Comment on this passage" });
  const named = { name: "Your comment on this text" };
  const field = margin.getByRole("textbox", named);
  const opened = margin.locator("form").filter({ has: page.getByRole("textbox", named) });
  await expect(field).toBeVisible();
  await expect(opened.getByText(`"${quote}"`)).toBeVisible();
  await expect(field).not.toBeFocused();
  const [cardBox, articleBox] = [await opened.boundingBox(), await article.boundingBox()];
  if (cardBox === null || articleBox === null) throw new Error("the card or article is not drawn");
  if (phone) expect(cardBox.y).toBeGreaterThan(articleBox.y);
  else expect(cardBox.x).toBeGreaterThanOrEqual(articleBox.x + articleBox.width);
  expect(await axe(page)).toEqual([]);

  await field.fill(comment);
  await expect(article.locator("mark.sel")).toHaveText(quote);
  await opened.getByRole("button", { name: "Comment" }).click();

  const card = page.locator("aside.margin article.card").filter({ hasText: comment });
  await expect(card).toBeVisible();
  await expect(card.getByText(`"${quote}"`)).toBeVisible();
  await expect(article.locator("mark.anchor").filter({ hasText: quote }).first()).toBeVisible();
  await expect(article.locator("mark.sel")).toHaveCount(0);

  await page.reload();
  const kept = page.locator("aside.margin article.card").filter({ hasText: comment });
  await expect(kept).toBeVisible();
  await expect(article.locator("mark.anchor").filter({ hasText: quote }).first()).toBeVisible();
  if (phone) {
    // Artboard 09: on a phone the card sits under the paragraph that holds
    // its text, inside the article.
    const before = await kept.evaluate(
      (element) => element.closest("aside.margin")?.previousElementSibling?.textContent ?? "",
    );
    expect(before).toContain(quote);
  } else {
    const [keptBox, nowBox] = [await kept.boundingBox(), await article.boundingBox()];
    if (keptBox === null || nowBox === null) throw new Error("the card or article is not drawn");
    expect(keptBox.x).toBeGreaterThanOrEqual(nowBox.x + nowBox.width);
  }

  expect(await axe(page)).toEqual([]);
  expect(violations).toEqual([]);
  expect(errors).toEqual([]);
  await page.screenshot({
    path: info.outputPath(`passage-${info.project.name}.png`),
    fullPage: true,
  });
});
