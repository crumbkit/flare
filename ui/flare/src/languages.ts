import type { LanguageInput } from "shiki/core";

// The languages Shiki colours on the reading page, each loaded as its own
// chunk only when a page has a block in it. docs/reading.md lists them;
// adding a language is one line here and one line there.
const grammars = {
  bash: () => import("@shikijs/langs/bash"),
  css: () => import("@shikijs/langs/css"),
  diff: () => import("@shikijs/langs/diff"),
  go: () => import("@shikijs/langs/go"),
  html: () => import("@shikijs/langs/html"),
  javascript: () => import("@shikijs/langs/javascript"),
  json: () => import("@shikijs/langs/json"),
  markdown: () => import("@shikijs/langs/markdown"),
  python: () => import("@shikijs/langs/python"),
  rust: () => import("@shikijs/langs/rust"),
  sql: () => import("@shikijs/langs/sql"),
  swift: () => import("@shikijs/langs/swift"),
  toml: () => import("@shikijs/langs/toml"),
  typescript: () => import("@shikijs/langs/typescript"),
  yaml: () => import("@shikijs/langs/yaml"),
} satisfies Record<string, () => LanguageInput>;

/** A language that the reading page colours. */
export type Language = keyof typeof grammars;

// Other names a fence may use for the same language.
const aliases: Record<string, Language> = {
  sh: "bash",
  shell: "bash",
  zsh: "bash",
  js: "javascript",
  ts: "typescript",
  py: "python",
  rs: "rust",
  yml: "yaml",
  md: "markdown",
};

function isLanguage(name: string): name is Language {
  return Object.hasOwn(grammars, name);
}

/** The language a fence's label names, or undefined when it is not one the page colours. */
export function languageOf(label: string): Language | undefined {
  const name = label.toLowerCase();
  if (isLanguage(name)) {
    return name;
  }
  return aliases[name];
}

/** Loads the grammar of `language`. */
export function grammar(language: Language): LanguageInput {
  return grammars[language]();
}
