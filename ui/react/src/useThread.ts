// The React binding of a comments section. It turns the thread's load state
// into props for the heading's status, the load-failure alert and the list,
// and keeps the posts sent from this page until a refetch returns them.
import {
  loadFailedMessage,
  loadingStatus,
  retryText,
  sectionHeading,
  threadSummary,
} from "@flare/headless/comments/a11y";
import {
  type Posted,
  type Thread,
  type ThreadLoad,
  withPosted,
} from "@flare/headless/comments/thread";
import { useCallback, useState } from "react";

export type ThreadView =
  | {
      readonly state: "loading";
      readonly statusProps: { readonly role: "status"; readonly children: string };
    }
  | {
      readonly state: "failed";
      readonly alertProps: { readonly role: "alert" };
      readonly message: string;
      readonly retryProps: {
        readonly type: "button";
        readonly onClick: () => void;
        readonly children: string;
      };
    }
  | { readonly state: "ready"; readonly thread: Thread; readonly summary: string };

export interface ThreadProps {
  readonly heading: string;
  readonly sectionProps: { readonly "aria-busy"?: true };
  readonly view: ThreadView;
  /** Adds a post this page sent, such as from a composer's onSent. */
  readonly add: (posted: Posted) => void;
}

export function useThread(load: ThreadLoad, onRetry: () => void): ThreadProps {
  const [sent, setSent] = useState<readonly Posted[]>([]);
  const add = useCallback((posted: Posted) => setSent((list) => [...list, posted]), []);

  let view: ThreadView;
  switch (load.state) {
    case "loading":
      view = { state: "loading", statusProps: { role: "status", children: loadingStatus } };
      break;
    case "failed":
      view = {
        state: "failed",
        alertProps: { role: "alert" },
        message: loadFailedMessage,
        retryProps: { type: "button", onClick: onRetry, children: retryText },
      };
      break;
    case "ready": {
      const thread = sent.reduce(withPosted, load.thread);
      view = { state: "ready", thread, summary: threadSummary(thread) };
      break;
    }
  }

  return {
    heading: sectionHeading,
    sectionProps: load.state === "loading" ? { "aria-busy": true } : {},
    view,
    add,
  };
}
