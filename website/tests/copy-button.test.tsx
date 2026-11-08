import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { CopyButton } from "@/components/CopyButton";

const command = "./scripts/setup-virtual-mic.sh";

function installClipboard(writeText: (text: string) => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
}

describe("CopyButton", () => {
  afterEach(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
    Object.defineProperty(document, "execCommand", {
      configurable: true,
      value: undefined,
    });
  });

  it("writes the exact command and announces the copied state", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    installClipboard(writeText);

    render(<CopyButton value={command} label="Copy command" />);
    fireEvent.click(screen.getByRole("button", { name: `Copy command: ${command}` }));

    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith(command);
    expect(await screen.findByText("Copied")).toBeVisible();
    expect(screen.getByRole("button")).toHaveAttribute("data-copied", "true");
    expect(screen.getByText("Command copied to clipboard.")).toBeInTheDocument();
  });

  it("falls back to a temporary textarea when the Clipboard API is unavailable", async () => {
    installClipboard(undefined as unknown as (text: string) => Promise<void>);
    const execCommand = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, "execCommand", {
      configurable: true,
      value: execCommand,
    });

    render(<CopyButton value={command} label="Copy" />);
    fireEvent.click(screen.getByRole("button", { name: `Copy: ${command}` }));

    expect(execCommand).toHaveBeenCalledWith("copy");
    expect(await screen.findByText("Copied")).toBeVisible();
  });

  it("reports a failure without pretending the command copied", async () => {
    installClipboard(vi.fn().mockRejectedValue(new Error("denied")));
    Object.defineProperty(document, "execCommand", {
      configurable: true,
      value: vi.fn().mockReturnValue(false),
    });

    render(<CopyButton value={command} label="Copy" />);
    fireEvent.click(screen.getByRole("button", { name: `Copy: ${command}` }));

    expect(await screen.findByText("Copy failed")).toBeVisible();
    expect(screen.getByRole("button")).toHaveAttribute("data-copied", "false");
    expect(screen.getByText(/Select the command and copy it manually/)).toBeInTheDocument();
  });
});
