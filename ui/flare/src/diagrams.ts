import type { MermaidConfig } from "mermaid";

// Mermaid's theme engine reads hex colours only, so these are Flare's
// colour tokens as hex, for the base theme. /evals/mermaid-styling-flare
// chose the base theme with these variables, the classic look and the ELK
// layout, and the signed mockup's Figure 1 is drawn with them.
const font = '"IBM Plex Sans", system-ui, sans-serif';

const light: MermaidConfig["themeVariables"] = {
  fontFamily: font,
  fontSize: "14px",
  background: "#f2f6f8",
  primaryColor: "#fcfeff",
  primaryTextColor: "#0c1318",
  primaryBorderColor: "#747b81",
  secondaryColor: "#cbeafb",
  secondaryTextColor: "#003455",
  secondaryBorderColor: "#cbeafb",
  tertiaryColor: "#edf1f3",
  tertiaryTextColor: "#464e55",
  tertiaryBorderColor: "#cbd2d7",
  lineColor: "#747b81",
  textColor: "#0c1318",
  mainBkg: "#fcfeff",
  nodeBorder: "#747b81",
  clusterBkg: "#edf1f3",
  clusterBorder: "#cbd2d7",
  titleColor: "#464e55",
  edgeLabelBackground: "#f2f6f8",
  nodeTextColor: "#0c1318",
};

const dark: MermaidConfig["themeVariables"] = {
  fontFamily: font,
  fontSize: "14px",
  darkMode: true,
  background: "#0b1014",
  primaryColor: "#161b20",
  primaryTextColor: "#e3e9ed",
  primaryBorderColor: "#616a71",
  secondaryColor: "#0f3a52",
  secondaryTextColor: "#c4e4f6",
  secondaryBorderColor: "#0f3a52",
  tertiaryColor: "#11171b",
  tertiaryTextColor: "#9da6ad",
  tertiaryBorderColor: "#2c343a",
  lineColor: "#616a71",
  textColor: "#e3e9ed",
  mainBkg: "#161b20",
  nodeBorder: "#616a71",
  clusterBkg: "#11171b",
  clusterBorder: "#2c343a",
  titleColor: "#9da6ad",
  edgeLabelBackground: "#0b1014",
  nodeTextColor: "#e3e9ed",
};

function config(themeVariables: MermaidConfig["themeVariables"]): MermaidConfig {
  return {
    startOnLoad: false,
    securityLevel: "strict",
    theme: "base",
    look: "classic",
    layout: "elk",
    darkMode: themeVariables?.darkMode === true,
    themeVariables,
  };
}

/** A figure the server wrote, with its source. */
interface Figure {
  figure: HTMLElement;
  source: HTMLPreElement;
}

function figures(root: ParentNode): Figure[] {
  const found: Figure[] = [];
  for (const figure of root.querySelectorAll<HTMLElement>("figure.diagram")) {
    const source = figure.querySelector("pre.diagram-source");
    if (source instanceof HTMLPreElement) {
      found.push({ figure, source });
    }
  }
  return found;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Shows Mermaid's message under a figure's source, which stays in view. */
function fail(figure: Figure, error: unknown): void {
  const note = figure.figure.ownerDocument.createElement("p");
  note.className = "diagram-error";
  note.textContent = `Mermaid could not draw this diagram: ${message(error)}`;
  figure.source.after(note);
  figure.figure.dataset.diagram = "failed";
}

// The smallest size a 14 px label may shrink to when a drawing is scaled
// to fit the column. A wider drawing keeps this size and scrolls in its box.
const SMALLEST_LABEL_PX = 10.5;
const LABEL_PX = 14;

/** The width below which `svg` would draw its labels under the smallest size. */
function floorWidth(svg: string, document: Document): number | undefined {
  const holder = document.createElement("div");
  holder.innerHTML = svg;
  const width = holder.querySelector("svg")?.viewBox.baseVal?.width;
  return width ? Math.ceil((width * SMALLEST_LABEL_PX) / LABEL_PX) : undefined;
}

/** Puts the light and the dark drawing in place of the figure's source. */
function show(figure: Figure, drawings: { light: string; dark: string }): void {
  const document = figure.figure.ownerDocument;
  const scroll = document.createElement("div");
  scroll.className = "scroll";
  // The box takes focus, so a keyboard can scroll a wide diagram.
  scroll.tabIndex = 0;
  const box = document.createElement("div");
  box.className = "mm";
  box.setAttribute("role", "img");
  const caption = figure.figure.querySelector("figcaption")?.textContent;
  box.setAttribute(
    "aria-label",
    caption ?? `${figure.source.dataset.diagram ?? "Mermaid"} diagram`,
  );
  for (const [name, svg] of [
    ["mm-light", drawings.light],
    ["mm-dark", drawings.dark],
  ] as const) {
    const holder = document.createElement("div");
    holder.className = name;
    holder.innerHTML = svg;
    box.append(holder);
  }
  const floor = floorWidth(drawings.light, document);
  if (floor !== undefined) {
    box.style.minWidth = `${floor}px`;
  }
  scroll.append(box);
  figure.source.before(scroll);
  figure.source.hidden = true;
  figure.figure.dataset.diagram = "drawn";
}

/**
 * Draws each Mermaid figure under `root` once for light and once for dark.
 * Mermaid loads only when the page has a figure. A figure Mermaid cannot
 * parse keeps its source and shows Mermaid's message; the others draw.
 */
export async function drawDiagrams(root: ParentNode): Promise<number> {
  const found = figures(root);
  if (found.length === 0) {
    return 0;
  }
  const { default: mermaid } = await import("mermaid");
  const drawn = new Map<Figure, { light: string; dark: string }>();
  for (const [index, figure] of found.entries()) {
    const source = figure.source.textContent;
    try {
      mermaid.initialize(config(light));
      const lit = await mermaid.render(`flare-mm-light-${index}`, source);
      mermaid.initialize(config(dark));
      const unlit = await mermaid.render(`flare-mm-dark-${index}`, source);
      drawn.set(figure, { light: lit.svg, dark: unlit.svg });
    } catch (error) {
      fail(figure, error);
    }
  }
  for (const [figure, drawings] of drawn) {
    show(figure, drawings);
  }
  return drawn.size;
}
