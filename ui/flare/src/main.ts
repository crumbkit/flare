import { wireCopyButtons } from "./copy.ts";
import { highlight } from "./highlight.ts";

// The reading page's script: it shows the Copy buttons and colours the
// code. The server's HTML is complete before this runs, and the server
// draws diagrams itself, so a failure leaves plain code. The body's
// data-reading attribute becomes "done" when the work ends.
wireCopyButtons(document);
await Promise.allSettled([highlight(document)]);
document.body.dataset.reading = "done";
