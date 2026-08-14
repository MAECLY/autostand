import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { HistoryViewToggle } from "@/components/history/HistoryViewToggle";
import { renderWithProviders } from "@/test/render";

describe("HistoryViewToggle", () => {
  it("marks the active view and reports a change", () => {
    const onChange = vi.fn();
    renderWithProviders(<HistoryViewToggle value="list" onChange={onChange} />);

    expect(screen.getByRole("tab", { name: "List" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    fireEvent.click(screen.getByRole("tab", { name: "Month" }));
    expect(onChange).toHaveBeenCalledWith("month");
  });
});
