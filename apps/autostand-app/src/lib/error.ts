/**
 * Normalisation for rejected `invoke` calls.
 *
 * Tauri rejects with the serde-serialized `AppError` (`{ code, message }`),
 * but a panic or a plugin failure can surface as a bare string or an `Error`,
 * so every rejection is funnelled through `toAppError` before it reaches the UI.
 * Contract: `docs/tauri/02-ipc-contracts.md` § Error handling.
 */

import { toast } from "sonner";

import type { AppError } from "@/lib/types";

/** Code used when the rejection did not carry a recognisable `AppError`. */
export const UNKNOWN_ERROR_CODE = "unknown";

export function isAppError(err: unknown): err is AppError {
  if (typeof err !== "object" || err === null) return false;
  const candidate = err as Partial<AppError>;
  return (
    typeof candidate.code === "string" && typeof candidate.message === "string"
  );
}

export function toAppError(err: unknown): AppError {
  if (isAppError(err)) return err;
  if (err instanceof Error) {
    return { code: UNKNOWN_ERROR_CODE, message: err.message };
  }
  if (typeof err === "string") {
    return { code: UNKNOWN_ERROR_CODE, message: err };
  }
  return { code: UNKNOWN_ERROR_CODE, message: String(err) };
}

/** Normalise a rejection and surface it as a toast. Returns the normalised error. */
export function handleInvokeError(err: unknown, ctx = ""): AppError {
  const appError = toAppError(err);
  const title = ctx ? `${ctx} — ${appError.code}` : appError.code;
  toast.error(title, { description: appError.message });
  return appError;
}
