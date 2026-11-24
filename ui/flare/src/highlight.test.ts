import { describe, expect, it } from "vitest";
import { highlight } from "./highlight.ts";

function page(label: string, code: string): HTMLElement {
  const root = document.createElement("article");
  root.innerHTML = `<div class="code"><div class="code-head"><span class="mono">${label}</span><button class="text-btn" type="button" data-copy hidden>Copy</button></div><pre><code class="language-${label}" data-lang="${label}">${code}</code></pre></div>`;
  return root;
}

describe("highlight", () => {
  it("colours a block in a known language with GitHub Light and GitHub Dark", async () => {
    const root = page("rust", "fn main() {\n    let x = 1;\n}");
    expect(await highlight(root)).toBe(1);
    const pre = root.querySelector("pre");
    expect(pre?.classList.contains("shiki")).toBe(true);
    const coloured = root.querySelectorAll("pre span[style*='--shiki-dark']");
    expect(coloured.length).toBeGreaterThan(0);
    expect(pre?.textContent).toBe("fn main() {\n    let x = 1;\n}");
    expect(root.querySelector("code")?.dataset.lang).toBe("rust");
    expect(root.querySelector(".code-head span")?.textContent).toBe("rust");
  });

  it("replaces the theme colours that fall below 4.5:1 contrast", async () => {
    const root = page("rust", "// a comment\nfn main() {}");
    await highlight(root);
    const html = root.innerHTML.toLowerCase();
    expect(html).toContain("#59636e");
    expect(html).toContain("--shiki-dark:#8b949e");
    expect(html).not.toContain("#6a737d");
    expect(html).not.toContain("#d73a49");
  });

  it("colours a block whose fence names an alias", async () => {
    const root = page("ts", "const x: number = 1;");
    expect(await highlight(root)).toBe(1);
    expect(root.querySelector("pre.shiki")).not.toBeNull();
  });

  it("leaves a block in an unknown language plain, with its label", async () => {
    const root = page("brainfuck", "++[&gt;+&lt;-]");
    const before = root.innerHTML;
    expect(await highlight(root)).toBe(0);
    expect(root.innerHTML).toBe(before);
    expect(root.querySelector("pre.shiki")).toBeNull();
    expect(root.querySelector(".code-head span")?.textContent).toBe("brainfuck");
  });

  it("does nothing on a page with no code block", async () => {
    const root = document.createElement("article");
    root.innerHTML = "<p>Text with <code>inline</code> code.</p>";
    expect(await highlight(root)).toBe(0);
  });
});
