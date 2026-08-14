import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { MonthGrid } from "@/components/history/MonthGrid";
import { renderWithProviders } from "@/test/render";

describe("MonthGrid", () => {
  it("marks the selected day and reports a click", () => {
    const onSelect = vi.fn();
    renderWithProviders(
      <MonthGrid
        anchor="2026-08-03"
        filed={new Set(["2026-08-03"])}
        selectedDate="2026-08-03"
        onSelect={onSelect}
      />,
    );

    const selected = screen.getByRole("button", { name: "2026-08-03" });
    expect(selected).toHaveAttribute("aria-current", "true");

    fireEvent.click(screen.getByRole("button", { name: "2026-08-10" }));
    expect(onSelect).toHaveBeenCalledWith("2026-08-10");
  });
});
