import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SignalSchematic } from "@/components/SignalSchematic";

describe("SignalSchematic", () => {
  it("switches the explained state and exposes it to assistive technology", async () => {
    const user = userEvent.setup();
    render(<SignalSchematic />);

    expect(screen.getByRole("button", { name: "Listening" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await user.click(screen.getByRole("button", { name: "Speaking" }));

    expect(screen.getByRole("button", { name: "Speaking" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByText(/DexVoice is playing the phrase/)).toHaveAttribute(
      "aria-live",
      "polite",
    );
    expect(screen.getByRole("button", { name: "Listening" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("is explicit that the schematic is not playing audio", () => {
    render(<SignalSchematic />);
    expect(
      screen.getByText("Interactive diagram. No audio is captured or played on this page."),
    ).toBeVisible();
  });
});
