import { grammar, type Language, languageOf } from "./languages.ts";

// Four of GitHub Light's token colours and one of GitHub Dark's fall below
// a 4.5:1 contrast on the code block's background. These replace them with
// GitHub's darker light colours and with GitHub Dark Default's comment
// colour, which the signed mockup draws.
const colorReplacements = {
  "github-light": {
    "#6a737d": "#59636e",
    "#d73a49": "#cf222e",
    "#22863a": "#116329",
    "#e36209": "#953800",
  },
  "github-dark": {
    "#6a737d": "#8b949e",
  },
};

/** A code block the server wrote, and the language it is in. */
interface Block {
  pre: HTMLPreElement;
  code: HTMLElement;
  language: Language;
}

function blocks(root: ParentNode): Block[] {
  const found: Block[] = [];
  for (const code of root.querySelectorAll<HTMLElement>(".code pre > code[data-lang]")) {
    const language = languageOf(code.dataset.lang ?? "");
    const pre = code.parentElement;
    if (language !== undefined && pre instanceof HTMLPreElement) {
      found.push({ pre, code, language });
    }
  }
  return found;
}

/**
 * Colours each code block under `root` whose language is in the list, with
 * GitHub Light and GitHub Dark. A block in any other language stays plain,
 * with its label. Shiki, its regex engine, the themes and each grammar load
 * only when a block needs them.
 */
export async function highlight(root: ParentNode): Promise<number> {
  const found = blocks(root);
  if (found.length === 0) {
    return 0;
  }
  const languages = [...new Set(found.map((block) => block.language))];
  const [{ createHighlighterCore }, { createJavaScriptRegexEngine }] = await Promise.all([
    import("shiki/core"),
    import("shiki/engine/javascript"),
  ]);
  const highlighter = await createHighlighterCore({
    themes: [import("@shikijs/themes/github-light"), import("@shikijs/themes/github-dark")],
    langs: languages.map(grammar),
    engine: createJavaScriptRegexEngine(),
  });
  for (const block of found) {
    const html = highlighter.codeToHtml(block.code.textContent, {
      lang: block.language,
      themes: { light: "github-light", dark: "github-dark" },
      colorReplacements,
    });
    const template = block.pre.ownerDocument.createElement("template");
    template.innerHTML = html;
    const coloured = template.content.firstElementChild;
    if (coloured === null) {
      continue;
    }
    coloured.querySelector("code")?.setAttribute("data-lang", block.code.dataset.lang ?? "");
    block.pre.replaceWith(coloured);
  }
  return found.length;
}
