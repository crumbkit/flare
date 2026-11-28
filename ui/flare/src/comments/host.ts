// The parts of the reading page that the comments draw on: the article, the
// margin beside it and the comments section after it. The server writes the
// article and the section; the margin is added here, so a page whose script
// does not run keeps the server's layout.

export interface ReadingHost {
  /** The article that holds the share's Markdown. */
  readonly article: HTMLElement;
  /** The margin beside the article, where cards on its text show on a wide page. */
  readonly margin: HTMLElement;
  /** The comments section after the article. */
  readonly section: HTMLElement;
  /** The version the page shows. */
  readonly version: number;
}

/**
 * The host of `document`'s reading page, with the article and a new margin
 * put side by side in a `.with-margin` box, or undefined for a page with no
 * Markdown article, such as an uploaded HTML share's.
 */
export function readingHost(document: Document): ReadingHost | undefined {
  const article = document.querySelector<HTMLElement>("main article.doc");
  const section = document.querySelector<HTMLElement>("section#comments[data-flare]");
  const version = Number(section?.dataset.version);
  if (article === null || section === null || !Number.isInteger(version) || version < 1) {
    return undefined;
  }
  let box = article.parentElement;
  if (!box?.classList.contains("with-margin")) {
    box = document.createElement("div");
    box.className = "with-margin";
    article.replaceWith(box);
    box.append(article);
  }
  let margin = box.querySelector<HTMLElement>(":scope > aside.margin");
  if (margin === null) {
    margin = document.createElement("aside");
    margin.className = "margin";
    box.append(margin);
  }
  return { article, margin, section, version };
}
