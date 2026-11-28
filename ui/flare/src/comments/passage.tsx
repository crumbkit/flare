// The card that composes a comment on the text a reader selected, as
// artboards 08 and 09 draw it: the quote, the labelled field, Cancel and
// Comment. It opens with focus left in the page, so copying still works.
// Escape in its field and Cancel close it.
import type { Anchor } from "@flare/headless/comments/anchor";
import { cancelText, quoteText } from "@flare/headless/comments/passage";
import type { Comment, Posted } from "@flare/headless/comments/thread";
import { useComposer } from "@flare/react/useComposer";
import { type KeyboardEvent, useEffect } from "react";
import { FailureAlert, MAX_LENGTH } from "./fields.tsx";
import { useSend } from "./send.ts";

const isComment = (posted: Posted): posted is Comment => posted.kind === "comment";

interface PassageCardProps {
  readonly flare: string;
  readonly owner: string;
  readonly anchor: Anchor;
  /** Runs with the field's text after each change. */
  readonly onText: (text: string) => void;
  /** Runs when the field gains focus. */
  readonly onFocus: () => void;
  readonly onClose: () => void;
  readonly onSent: (posted: Comment) => void;
}

export function PassageCard({
  flare,
  owner,
  anchor,
  onText,
  onFocus,
  onClose,
  onSent,
}: PassageCardProps) {
  const send = useSend(flare, undefined, isComment, anchor);
  const composer = useComposer({
    target: { kind: "passage" },
    send,
    maxLength: MAX_LENGTH,
    owner,
    onSent,
  });
  const text = "draft" in composer.state ? composer.state.draft.text : "";
  useEffect(() => onText(text), [onText, text]);
  const { onKeyDown, ...field } = composer.fieldProps;
  const keyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    onKeyDown(event);
  };
  return (
    <form className="card" {...composer.formProps}>
      <div className="quote">{quoteText(anchor.quote)}</div>
      <label className="lab" htmlFor={field.id}>
        {composer.label}
      </label>
      <textarea rows={3} {...field} onKeyDown={keyDown} onFocus={onFocus} />
      {composer.blockedReason && (
        <p className="cap" id={composer.blockedReason.id}>
          {composer.blockedReason.text}
        </p>
      )}
      <FailureAlert failure={composer.failure} />
      {composer.submitProps && (
        <div className="actions">
          <button className="ghost" type="button" onClick={onClose}>
            {cancelText}
          </button>
          <button className="btn" {...composer.submitProps} />
        </div>
      )}
      <div className="visually-hidden" {...composer.statusProps} />
    </form>
  );
}
