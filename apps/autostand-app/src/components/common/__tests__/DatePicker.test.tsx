import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { DatePicker } from "@/components/common/DatePicker";
import { renderWithProviders } from "@/test/render";

describe("DatePicker", () => {
  it("opens a calendar dialog and commits a day", () => {
    const onChange = vi.fn();
    renderWithProviders(
      <DatePicker id="filing-date" value="2026-08-03" onChange={onChange} />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Aug 3, 2026/i }));
    expect(screen.getByRole("dialog", { name: "Pick a date" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "2026-08-10" }));
    expect(onChange).toHaveBeenCalledWith("2026-08-10");
  });
});
