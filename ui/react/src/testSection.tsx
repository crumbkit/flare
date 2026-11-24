// A comments section built from the binding the way mockup
// LRJsZhYqKPHcug6pQKw8dA draws it, with no styling. Only the tests import it.
import type { Draft, DraftKey, SendResult } from "@flare/headless/comments/composer";
import { isDraftKey } from "@flare/headless/comments/composer";
import {
  type Author,
  type Comment,
  type CommentId,
  isCommentId,
  isReplyId,
  type Reply,
  type ThreadLoad,
} from "@flare/headless/comments/thread";
import { useComposer } from "./useComposer.ts";
import { useThread } from "./useThread.ts";

export function keys(): () => DraftKey {
  let next = 0;
  return () => {
    next += 1;
    const key = `key-${next}`;
    if (!isDraftKey(key)) throw new Error("unreachable");
    return key;
  };
}

export function commentId(raw: string): CommentId {
  if (!isCommentId(raw)) throw new Error(`not a comment id: ${raw}`);
  return raw;
}

export function postedReply(parent: CommentId, author: Author, body: string): Reply {
  const id = `reply-${body.length}`;
  if (!isReplyId(id)) throw new Error("unreachable");
  return { kind: "reply", id, parent, author, body, at: "2026-11-23T10:00:00Z" };
}

export const tomas: Author = { name: "Tomás Rivera" };
export const jonas: Author = { name: "Jonas Weber" };

export function mockupThread(): readonly Comment[] {
  const c1 = commentId("c1");
  return [
    {
      kind: "comment",
      id: c1,
      author: tomas,
      body: "Can the table also show which vessels carry vehicles?",
      at: "2026-11-21T16:02:00Z",
      deleted: false,
      replies: [postedReply(c1, { name: "Ana Okafor" }, "Yes. All three do.")],
    },
    {
      kind: "comment",
      id: commentId("c2"),
      author: jonas,
      body: "Do we pay for a night's stay?",
      at: "2026-11-22T11:27:00Z",
      deleted: false,
      replies: [],
    },
  ];
}

export type SendReply = (draft: Draft) => Promise<SendResult<Reply>>;
export type SendComment = (draft: Draft) => Promise<SendResult<Comment>>;

interface ReplyFieldProps {
  readonly parent: Comment;
  readonly send: SendReply;
  readonly onSent: (posted: Reply) => void;
}

export function ReplyField({ parent, send, onSent }: ReplyFieldProps) {
  const composer = useComposer({
    target: { kind: "reply", to: parent.author },
    send,
    maxLength: 2000,
    owner: "Ana Okafor",
    newKey: keys(),
    onSent,
  });
  return (
    <>
      <div>
        <input type="text" {...composer.fieldProps} />
        {composer.submitProps && <button {...composer.submitProps} />}
      </div>
      {composer.failure && (
        <div {...composer.failure.alertProps}>
          <p>{composer.failure.message}</p>
          <button {...composer.failure.actionProps} />
        </div>
      )}
      <div {...composer.statusProps} />
    </>
  );
}

interface PageFieldProps {
  readonly send: SendComment;
  readonly threadIsEmpty: boolean;
  readonly maxLength?: number;
  readonly onSent?: (posted: Comment) => void;
}

export function PageField({ send, threadIsEmpty, maxLength = 2000, onSent }: PageFieldProps) {
  const composer = useComposer({
    target: { kind: "comment" },
    send,
    maxLength,
    owner: "Ana Okafor",
    threadIsEmpty,
    newKey: keys(),
    ...(onSent ? { onSent } : {}),
  });
  return (
    <form {...composer.formProps}>
      <label htmlFor={composer.fieldProps.id}>{composer.label}</label>
      <textarea rows={3} {...composer.fieldProps} />
      {composer.blockedReason && (
        <p id={composer.blockedReason.id}>{composer.blockedReason.text}</p>
      )}
      {composer.failure && (
        <div {...composer.failure.alertProps}>
          <p>{composer.failure.message}</p>
          <button {...composer.failure.actionProps} />
        </div>
      )}
      {composer.submitProps && (
        <div>
          {composer.hint && <span>{composer.hint}</span>}
          {composer.audience && <span>{composer.audience}</span>}
          <button {...composer.submitProps} />
        </div>
      )}
      <div {...composer.statusProps} />
    </form>
  );
}

interface SectionProps {
  readonly load: ThreadLoad;
  readonly onRetry: () => void;
  readonly sendReply: SendReply;
  readonly sendComment: SendComment;
}

export function Section({ load, onRetry, sendReply, sendComment }: SectionProps) {
  const thread = useThread(load, onRetry);
  const { view } = thread;
  return (
    <section {...thread.sectionProps}>
      <div>
        <h2>{thread.heading}</h2>
        {view.state === "loading" && <span {...view.statusProps} />}
        {view.state === "ready" && <span>{view.summary}</span>}
      </div>
      {view.state === "failed" && (
        <div {...view.alertProps}>
          <p>{view.message}</p>
          <button {...view.retryProps} />
        </div>
      )}
      {view.state === "ready" && (
        <>
          {view.thread.map((comment) => (
            <article key={comment.id}>
              <b>{comment.author.name}</b>
              <p>{comment.body}</p>
              {comment.replies.map((reply) => (
                <div key={reply.id}>
                  <b>{reply.author.name}</b>
                  <p>{reply.body}</p>
                </div>
              ))}
              <ReplyField parent={comment} send={sendReply} onSent={thread.add} />
            </article>
          ))}
          <PageField
            send={sendComment}
            threadIsEmpty={view.thread.length === 0}
            onSent={thread.add}
          />
        </>
      )}
    </section>
  );
}
