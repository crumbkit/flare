import { describe, expect, it } from "vitest";
import { drawDiagrams } from "./diagrams.ts";

function figure(source: string): string {
  return `<figure class="diagram"><pre class="diagram-source" data-diagram="flowchart">${source}</pre><figcaption>Figure 1. A caption.</figcaption></figure>`;
}

describe("drawDiagrams", () => {
  it("keeps a broken diagram's source in view and shows Mermaid's message", async () => {
    const root = document.createElement("article");
    root.innerHTML = figure("flowchart LR\n  a --&gt;&gt;&gt; [[[ b");
    expect(await drawDiagrams(root)).toBe(0);
    const source = root.querySelector<HTMLPreElement>("pre.diagram-source");
    expect(source?.hidden).toBe(false);
    expect(source?.textContent).toBe("flowchart LR\n  a -->>> [[[ b");
    const note = root.querySelector(".diagram-error");
    expect(note?.textContent).toMatch(/^Mermaid could not draw this diagram: /);
    expect(root.querySelector<HTMLElement>("figure")?.dataset.diagram).toBe("failed");
    expect(root.querySelector("svg")).toBeNull();
  });

  it("does nothing on a page with no figure", async () => {
    const root = document.createElement("article");
    root.innerHTML = "<pre><code>flowchart LR</code></pre>";
    expect(await drawDiagrams(root)).toBe(0);
  });
});
