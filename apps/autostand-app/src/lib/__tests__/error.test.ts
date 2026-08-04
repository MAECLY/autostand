import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  },
}));

import { toast } from "sonner";

import {
  UNKNOWN_ERROR_CODE,
  handleInvokeError,
  isAppError,
  toAppError,
} from "@/lib/error";

const toastError = vi.mocked(toast.error);

beforeEach(() => {
  vi.clearAllMocks();
});

describe("isAppError", () => {
  it("accepts the serialized AppError shape", () => {
    expect(isAppError({ code: "config", message: "missing dailies_dir" })).toBe(
      true,
    );
  });

  it("accepts extra fields alongside the required two", () => {
    expect(isAppError({ code: "io", message: "boom", detail: 1 })).toBe(true);
  });

  it("rejects anything missing a string code or message", () => {
    expect(isAppError({ code: "io" })).toBe(false);
    expect(isAppError({ message: "io" })).toBe(false);
    expect(isAppError({ code: 42, message: "io" })).toBe(false);
    expect(isAppError({ code: "io", message: 42 })).toBe(false);
  });

  it("rejects non-objects and null", () => {
    expect(isAppError(null)).toBe(false);
    expect(isAppError(undefined)).toBe(false);
    expect(isAppError("io error")).toBe(false);
    expect(isAppError(42)).toBe(false);
    expect(isAppError(new Error("boom"))).toBe(false);
  });
});

describe("toAppError", () => {
  it("passes an AppError through untouched", () => {
    const appError = { code: "lock", message: "another run holds the lock" };
    expect(toAppError(appError)).toBe(appError);
  });

  it("maps an Error to its message under the unknown code", () => {
    expect(toAppError(new Error("spawn ENOENT"))).toEqual({
      code: UNKNOWN_ERROR_CODE,
      message: "spawn ENOENT",
    });
  });

  it("maps a subclass of Error the same way", () => {
    expect(toAppError(new TypeError("not a function"))).toEqual({
      code: UNKNOWN_ERROR_CODE,
      message: "not a function",
    });
  });

  it("maps a bare string rejection", () => {
    expect(toAppError("panicked at 'unwrap on None'")).toEqual({
      code: UNKNOWN_ERROR_CODE,
      message: "panicked at 'unwrap on None'",
    });
  });

  it("stringifies anything else", () => {
    expect(toAppError(42).message).toBe("42");
    expect(toAppError(null).message).toBe("null");
    expect(toAppError(undefined).message).toBe("undefined");
    expect(toAppError({ nope: true }).message).toBe("[object Object]");
    expect(toAppError(undefined).code).toBe(UNKNOWN_ERROR_CODE);
  });
});

describe("handleInvokeError", () => {
  it("returns the normalised error", () => {
    expect(handleInvokeError(new Error("boom"))).toEqual({
      code: UNKNOWN_ERROR_CODE,
      message: "boom",
    });
  });

  it("toasts the code with the message as the description", () => {
    handleInvokeError({ code: "config", message: "missing dailies_dir" });

    expect(toastError).toHaveBeenCalledTimes(1);
    expect(toastError).toHaveBeenCalledWith("config", {
      description: "missing dailies_dir",
    });
  });

  it("prefixes the toast title with the caller's context", () => {
    handleInvokeError({ code: "config", message: "missing dailies_dir" }, "Save settings");

    expect(toastError).toHaveBeenCalledWith("Save settings — config", {
      description: "missing dailies_dir",
    });
  });

  it("still toasts for a non-AppError rejection", () => {
    handleInvokeError("raw string rejection", "Compile");

    expect(toastError).toHaveBeenCalledWith(`Compile — ${UNKNOWN_ERROR_CODE}`, {
      description: "raw string rejection",
    });
  });
});
