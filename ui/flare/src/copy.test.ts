import { describe, expect, it, vi } from "vitest";
import { wireCopyButtons } from "./copy.ts";

describe("wireCopyButtons", () => {
  it("shows the hidden Copy button and copies the block's text", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const root = document.createElement("article");
    root.innerHTML =
      '<div class="code"><div class="code-head"><span>rust</span><button type="button" data-copy hidden>Copy</button></div><pre><code>fn x() {}</code></pre></div>';
    wireCopyButtons(root);
    const button = root.querySelector("button");
    expect(button?.hidden).toBe(false);
    button?.click();
    await vi.waitFor(() => expect(button?.textContent).toBe("Copied"));
    expect(writeText).toHaveBeenCalledWith("fn x() {}");
  });
});
