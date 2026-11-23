// The React binding of one comment or reply field. It keeps a headless
// composer for the life of the component and turns its state into props for
// the label, the field, the send button, the failure alert and the live
// region. It renders nothing, so the app styles every element itself.
import {
  failureAction,
  failureMessage,
  fieldLabel,
  placeholder,
  sendHint,
  sendsOnKey,
  sentAnnouncement,
  submitText,
  tooLongReason,
} from "@flare/headless/comments/a11y";
import {
  blocker,
  type ComposerState,
  createComposer,
  type DraftKey,
  isDraftKey,
  type Send,
} from "@flare/headless/comments/composer";
import type { Target } from "@flare/headless/comments/thread";
import {
  type ChangeEvent,
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useId,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

type FieldElement = HTMLTextAreaElement | HTMLInputElement;

export interface UseComposerOptions<P> {
  readonly target: Target;
  readonly send: Send<P>;
  /** The most characters a post may hold. */
  readonly maxLength: number;
  /** The person a refused writer asks for access, named in the refusal. */
  readonly owner: string;
  /** Whether the thread has no comments yet, which changes the page field's placeholder. */
  readonly threadIsEmpty?: boolean;
  /** Runs once for each post the server stored, such as to add it to the thread. */
  readonly onSent?: (posted: P) => void;
  /** Makes a draft's idempotency key. The default is a random UUID. */
  readonly newKey?: () => DraftKey;
}

export interface ComposerProps<P> {
  readonly state: ComposerState<P>;
  readonly formProps: { readonly onSubmit: (event: FormEvent) => void };
  /** The field's name. The page field shows it in a label for fieldProps.id; a reply field carries it as aria-label. */
  readonly label: string;
  readonly fieldProps: {
    readonly id: string;
    readonly ref: (element: FieldElement | null) => void;
    readonly value: string;
    readonly onChange: (event: ChangeEvent<FieldElement>) => void;
    readonly onKeyDown: (event: KeyboardEvent<FieldElement>) => void;
    readonly readOnly: boolean;
    readonly placeholder: string;
    readonly "aria-label"?: string;
    readonly "aria-describedby"?: string;
  };
  /** The send button's props, or undefined after a refusal, when nothing can be sent. */
  readonly submitProps:
    | {
        readonly type: "submit" | "button";
        readonly onClick?: () => void;
        readonly children: string;
        readonly "aria-disabled"?: true;
        readonly "aria-busy"?: true;
      }
    | undefined;
  /** The page field's send hint while it holds text. */
  readonly hint: string | undefined;
  /** Why the text cannot be sent, when it is over the limit. */
  readonly blockedReason: { readonly id: string; readonly text: string } | undefined;
  readonly failure:
    | {
        readonly alertProps: { readonly role: "alert"; readonly id: string };
        readonly message: string;
        readonly actionProps: {
          readonly type: "button";
          readonly onClick: () => void;
          readonly children: string;
        };
      }
    | undefined;
  /** A live region that is always rendered, so a screen reader hears what it later says. */
  readonly statusProps: { readonly role: "status"; readonly children: string };
}

function randomKey(): DraftKey {
  const key = crypto.randomUUID();
  if (!isDraftKey(key)) throw new Error("crypto.randomUUID returned an empty string");
  return key;
}

function textOf<P>(state: ComposerState<P>): string {
  return "draft" in state ? state.draft.text : "";
}

export function useComposer<P>(options: UseComposerOptions<P>): ComposerProps<P> {
  const { target, maxLength } = options;
  const sendRef = useRef(options.send);
  const onSentRef = useRef(options.onSent);
  useEffect(() => {
    sendRef.current = options.send;
    onSentRef.current = options.onSent;
  });
  const [composer] = useState(() =>
    createComposer<P>({
      send: (draft) => sendRef.current(draft),
      newKey: options.newKey ?? randomKey,
      maxLength,
    }),
  );
  const state = useSyncExternalStore(composer.subscribe, composer.getState);
  const field = useRef<FieldElement | null>(null);
  const fieldId = useId();
  const alertId = useId();
  const reasonId = useId();

  // A sent post is reported once, and the field keeps focus for the next one.
  useEffect(() => {
    if (state.status !== "sent") return;
    onSentRef.current?.(state.posted);
    field.current?.focus();
  }, [state]);

  // Leaving the page with unsent text asks first.
  const unsent =
    state.status === "composing" || state.status === "sending" || state.status === "failed";
  useEffect(() => {
    if (!unsent) return;
    const ask = (event: Event) => event.preventDefault();
    window.addEventListener("beforeunload", ask);
    return () => window.removeEventListener("beforeunload", ask);
  }, [unsent]);

  const submit = () => void composer.submit();
  const blocked = blocker(state, maxLength);
  const sending = state.status === "sending";
  const failure = state.status === "failed" ? state : undefined;
  const reason =
    blocked === "too-long"
      ? { id: reasonId, text: tooLongReason([...textOf(state)].length, maxLength) }
      : undefined;
  let describedBy: string | undefined;
  if (failure) describedBy = alertId;
  else if (reason) describedBy = reason.id;
  // The mockup leaves an empty reply field's button active; pressing it does nothing.
  const inactive = blocked !== undefined && !(target.kind === "reply" && blocked === "empty");

  return {
    state,
    formProps: {
      onSubmit: (event) => {
        event.preventDefault();
        submit();
      },
    },
    label: fieldLabel(target),
    fieldProps: {
      id: fieldId,
      ref: (element) => {
        field.current = element;
      },
      value: textOf(state),
      onChange: (event) => composer.edit(event.currentTarget.value),
      onKeyDown: (event) => {
        const press = {
          key: event.key,
          ctrlKey: event.ctrlKey,
          metaKey: event.metaKey,
          isComposing: event.nativeEvent.isComposing,
        };
        if (!sendsOnKey(target, press)) return;
        event.preventDefault();
        submit();
      },
      readOnly: sending,
      placeholder: placeholder(target, options.threadIsEmpty ?? false),
      ...(target.kind === "reply" ? { "aria-label": fieldLabel(target) } : {}),
      ...(describedBy ? { "aria-describedby": describedBy } : {}),
    },
    submitProps:
      failure?.failure === "refused"
        ? undefined
        : {
            ...(target.kind === "comment"
              ? { type: "submit" as const }
              : { type: "button" as const, onClick: submit }),
            children: submitText(target, sending),
            ...(inactive ? { "aria-disabled": true as const } : {}),
            ...(sending ? { "aria-busy": true as const } : {}),
          },
    hint: target.kind === "comment" && state.status === "composing" ? sendHint : undefined,
    blockedReason: reason,
    failure: failure && {
      alertProps: { role: "alert", id: alertId },
      message: failureMessage(target, failure.failure, options.owner),
      actionProps: {
        type: "button",
        children: failureAction(failure.failure),
        onClick:
          failure.failure === "refused"
            ? () => void navigator.clipboard.writeText(failure.draft.text)
            : submit,
      },
    },
    statusProps: {
      role: "status",
      children: state.status === "sent" ? sentAnnouncement(target) : "",
    },
  };
}
