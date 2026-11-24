import { wireCopyButtons } from "./copy.ts";
import { drawDiagrams } from "./diagrams.ts";
import { highlight } from "./highlight.ts";

// The reading page's script: it shows the Copy buttons, colours the code
// and draws the diagrams. The server's HTML is complete before this runs,
// so a failure leaves plain code and diagram source. The body's
// data-reading attribute becomes "done" when the work ends.
wireCopyButtons(document);
await Promise.allSettled([highlight(document), drawDiagrams(document)]);
document.body.dataset.reading = "done";
