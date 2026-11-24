// The two fields of the comments section, as the comment parts mockup draws
// them: the page field under the thread (artboards 01 to 05) and the
// one-line reply field under each top-level comment. The React binding
// gives every label, state and word; these components only lay them out.
import type { Comment, Posted, Reply } from "@flare/headless/comments/thread";
import { type ComposerProps, useComposer } from "@flare/react/useComposer";
import { useSend } from "./send.ts";

/** The most characters a post may hold, as flare-server's comment store allows. */
const MAX_LENGTH = 8000;

const isComment = (posted: Posted): posted is Comment => posted.kind === "comment";
const isReply = (posted: Posted): posted is Reply => posted.kind === "reply";

function FailureAlert<P>({ failure }: { readonly failure: ComposerProps<P>["failure"] }) {
  if (failure === undefined) return null;
  const tone = failure.actionProps.children === "Retry" ? "amber" : "red";
  return (
    <div className={`alert ${tone}`} {...failure.alertProps}>
      <p>{failure.message}</p>
      <button className="ghost" {...failure.actionProps} />
    </div>
  );
}

interface PageFieldProps {
  readonly flare: string;
  readonly owner: string;
  readonly threadIsEmpty: boolean;
  readonly onSent: (posted: Comment) => void;
}

export function PageField({ flare, owner, threadIsEmpty, onSent }: PageFieldProps) {
  const send = useSend(flare, undefined, isComment);
  const composer = useComposer({
    target: { kind: "comment" },
    send,
    maxLength: MAX_LENGTH,
    owner,
    threadIsEmpty,
    onSent,
  });
  // Artboard 01 draws the empty section's idle field with no audience line.
  const audience =
    threadIsEmpty && composer.state.status === "idle" ? undefined : composer.audience;
  return (
    <form className="compose" {...composer.formProps}>
      <label htmlFor={composer.fieldProps.id}>{composer.label}</label>
      <textarea rows={3} {...composer.fieldProps} />
      {composer.blockedReason && (
        <p className="cap" id={composer.blockedReason.id}>
          {composer.blockedReason.text}
        </p>
      )}
      <FailureAlert failure={composer.failure} />
      {composer.submitProps && (
        <div className="actions">
          {composer.hint && <span className="cap spread">{composer.hint}</span>}
          {audience && <span className="cap">{audience}</span>}
          <button className="btn" {...composer.submitProps} />
        </div>
      )}
      <div className="visually-hidden" {...composer.statusProps} />
    </form>
  );
}

interface ReplyFieldProps {
  readonly flare: string;
  readonly owner: string;
  readonly parent: Comment;
  readonly onSent: (posted: Reply) => void;
}

export function ReplyField({ flare, owner, parent, onSent }: ReplyFieldProps) {
  const send = useSend(flare, parent.id, isReply);
  const composer = useComposer({
    target: { kind: "reply", to: parent.author },
    send,
    maxLength: MAX_LENGTH,
    owner,
    onSent,
  });
  return (
    <>
      <div className="field-row">
        <input type="text" {...composer.fieldProps} />
        {composer.submitProps && <button className="ghost" {...composer.submitProps} />}
      </div>
      {composer.blockedReason && (
        <p className="cap" id={composer.blockedReason.id}>
          {composer.blockedReason.text}
        </p>
      )}
      <FailureAlert failure={composer.failure} />
      <div className="visually-hidden" {...composer.statusProps} />
    </>
  );
}
