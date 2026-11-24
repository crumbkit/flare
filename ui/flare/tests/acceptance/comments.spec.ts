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
  await expect(page.getByRole("link", { name: /^(No comments|\d+ comments?)$/ })).toHaveAttribute(
    "href",
    "#comments",
  );
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

  expect(await axe(page)).toEqual([]);
  expect(violations).toEqual([]);
  expect(errors).toEqual([]);
  await page.screenshot({
    path: info.outputPath(`comments-${info.project.name}.png`),
    fullPage: true,
  });
});
