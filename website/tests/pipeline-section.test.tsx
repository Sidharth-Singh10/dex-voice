import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PipelineSection } from "@/components/PipelineSection";

describe("PipelineSection", () => {
  it("changes the stage panel when a stage tab is selected", async () => {
    const user = userEvent.setup();
    render(<PipelineSection />);

    expect(screen.getByRole("tab", { name: /Mic/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveTextContent("A physical microphone");

    await user.click(screen.getByRole("tab", { name: /Phonon/ }));

    expect(screen.getByRole("tab", { name: /Phonon/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Phonon speaks it back");
    expect(screen.getByText("Phonon 31M on CPU")).toBeInTheDocument();
  });

  it("moves through stages with the arrow keys", async () => {
    const user = userEvent.setup();
    render(<PipelineSection />);

    const micTab = screen.getByRole("tab", { name: /Mic/ });
    micTab.focus();
    await user.keyboard("{ArrowRight}");

    expect(screen.getByRole("tab", { name: /Parakeet/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Parakeet commits words live");
  });
});
