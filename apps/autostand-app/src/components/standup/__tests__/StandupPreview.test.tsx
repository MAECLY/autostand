import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { StandupPreview } from "@/components/standup/StandupPreview";
import {
  FIXTURE_DATE,
  FIXTURE_HOST,
  FIXTURE_OTHER_HOST,
  makeStandupFileContent,
} from "@/test/mocks";

/** The AUTO/MANUAL cards are the only `.rounded-lg.border` boxes on screen. */
function cards(container: HTMLElement): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>("div.rounded-lg.border")];
}

describe("StandupPreview", () => {
  it("renders the title and one card per AUTO block plus the MANUAL card", () => {
    const { container } = render(
      <StandupPreview content={makeStandupFileContent()} />,
    );

    expect(
      screen.getByRole("heading", { name: "Daily Standup — August 03, 2026" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Work completed August 01–02, 2026/)).toBeInTheDocument();

    // Two AUTO blocks + the single MANUAL region.
    expect(cards(container)).toHaveLength(3);
    expect(screen.getAllByText("Auto")).toHaveLength(2);
    expect(screen.getByText(FIXTURE_HOST)).toBeInTheDocument();
    expect(screen.getByText(FIXTURE_OTHER_HOST)).toBeInTheDocument();
  });

  it("renders each host's body as markdown inside its own card", () => {
    const { container } = render(
      <StandupPreview content={makeStandupFileContent()} />,
    );

    const [first, second] = cards(container);
    expect(within(first).getByText("FIF-136 wired the pipeline").tagName).toBe("LI");
    expect(within(second).getByText("FIF-141 drafted the discovery KB")).toBeInTheDocument();
    expect(within(first).queryByText("FIF-141 drafted the discovery KB")).toBeNull();
  });

  it("highlights only the local host's card", () => {
    const { container } = render(
      <StandupPreview content={makeStandupFileContent()} hostSlug={FIXTURE_HOST} />,
    );

    const highlighted = container.querySelectorAll(".ring-ring");
    expect(highlighted).toHaveLength(1);
    expect(highlighted[0]).toHaveTextContent(FIXTURE_HOST);
    expect(highlighted[0]).not.toHaveTextContent(FIXTURE_OTHER_HOST);
  });

  it("highlights nothing when this machine has not filed", () => {
    const { container } = render(
      <StandupPreview content={makeStandupFileContent()} hostSlug="unknown-host" />,
    );

    expect(container.querySelectorAll(".ring-ring")).toHaveLength(0);
  });

  it("renders the MANUAL region with its never-overwritten marker", () => {
    render(<StandupPreview content={makeStandupFileContent()} />);

    expect(screen.getByText("Manual")).toBeInTheDocument();
    expect(screen.getByText("never overwritten")).toBeInTheDocument();
    expect(
      screen.getByText("Pairing session with the platform team"),
    ).toBeInTheDocument();
  });

  it("omits the MANUAL card when the region holds only the placeholder comment", () => {
    render(
      <StandupPreview
        content={makeStandupFileContent({
          manual_region: "<!-- add manual items below -->",
        })}
      />,
    );

    expect(screen.queryByText("Manual")).toBeNull();
    expect(screen.queryByText(/add manual items below/)).toBeNull();
    expect(screen.getAllByText("Auto")).toHaveLength(2);
  });

  it("shows the empty state when the file has no AUTO blocks and no manual items", () => {
    render(
      <StandupPreview
        content={makeStandupFileContent({
          auto_blocks: [],
          manual_region: "<!-- add manual items below -->",
        })}
      />,
    );

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(`Nothing filed for ${FIXTURE_DATE}`);
    expect(alert).toHaveTextContent(
      "Compile a standup or add a manual item to populate this file.",
    );
    expect(screen.queryByText("Auto")).toBeNull();
  });

  it("still renders when only the MANUAL region has content", () => {
    render(
      <StandupPreview
        content={makeStandupFileContent({ auto_blocks: [], manual_region: "- solo note" })}
      />,
    );

    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Manual")).toBeInTheDocument();
    expect(screen.getByText("solo note")).toBeInTheDocument();
  });
});
