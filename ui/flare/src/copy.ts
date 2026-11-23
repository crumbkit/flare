const COPIED_FOR_MS = 1500;

/**
 * Shows each code block's Copy button under `root` and makes it copy the
 * block's text. The server writes the buttons hidden, so a page whose
 * script does not run shows none.
 */
export function wireCopyButtons(root: ParentNode): void {
  for (const button of root.querySelectorAll<HTMLButtonElement>(".code button[data-copy]")) {
    const block = button.closest(".code");
    button.hidden = false;
    button.addEventListener("click", () => {
      const text = block?.querySelector("pre")?.textContent ?? "";
      navigator.clipboard.writeText(text).then(
        () => {
          button.textContent = "Copied";
          setTimeout(() => {
            button.textContent = "Copy";
          }, COPIED_FOR_MS);
        },
        () => {
          button.textContent = "Copy failed";
        },
      );
    });
  }
}
