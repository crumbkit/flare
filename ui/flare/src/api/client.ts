// The network owner of the comments section (BASE-7). Every request goes
// through `request`, which sends the session cookie to this origin, parses
// the answer with the caller's schema and throws an ApiError for every
// failure, because TanStack Query takes a thrown error as a query's or a
// mutation's failure (BASE-23).
import { z } from "zod";

// zod compiles a faster parser with `new Function` unless it runs jitless.
// The reading page's policy has no 'unsafe-eval', so the attempt would be
// reported as a policy violation; jitless parsing never makes it. The
// network owner sets it because every answer is parsed here.
z.config({ jitless: true });

/**
 * Why a request failed. A refusal (403 or 404), a full flare (409), an
 * ended session (401) and invalid text (400) are answers the server will
 * repeat, so nothing retries them.
 */
export type ApiFailure =
  | "no-answer"
  | "refused"
  | "full"
  | "signed-out"
  | "invalid"
  | "failed"
  | "malformed";

export class ApiError extends Error {
  readonly kind: ApiFailure;
  readonly status: number | undefined;

  constructor(kind: ApiFailure, status?: number, options?: ErrorOptions) {
    super(status === undefined ? kind : `${kind} (${status})`, options);
    this.name = "ApiError";
    this.kind = kind;
    this.status = status;
  }
}

function failureOf(status: number): ApiFailure {
  switch (status) {
    case 400:
      return "invalid";
    case 401:
      return "signed-out";
    case 403:
    case 404:
      return "refused";
    case 409:
      return "full";
    default:
      return "failed";
  }
}

/** Sends `init` to `path` on this origin and parses the answer with `schema`. */
export async function request<T>(
  path: string,
  schema: z.ZodType<T>,
  init: RequestInit = {},
): Promise<T> {
  let response: Response;
  try {
    response = await fetch(path, { ...init, credentials: "same-origin" });
  } catch (cause) {
    throw new ApiError("no-answer", undefined, { cause });
  }
  if (!response.ok) throw new ApiError(failureOf(response.status), response.status);
  let body: unknown;
  try {
    body = await response.json();
  } catch (cause) {
    throw new ApiError("malformed", response.status, { cause });
  }
  const parsed = schema.safeParse(body);
  if (!parsed.success) {
    throw new ApiError("malformed", response.status, { cause: parsed.error });
  }
  return parsed.data;
}

/**
 * Reads the text at `path` on this origin, such as a reading page's HTML,
 * and throws an ApiError for every failure, as `request` does.
 */
export async function requestText(path: string): Promise<string> {
  let response: Response;
  try {
    response = await fetch(path, { credentials: "same-origin" });
  } catch (cause) {
    throw new ApiError("no-answer", undefined, { cause });
  }
  if (!response.ok) throw new ApiError(failureOf(response.status), response.status);
  try {
    return await response.text();
  } catch (cause) {
    throw new ApiError("malformed", response.status, { cause });
  }
}
