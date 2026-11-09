import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SiteNav } from "@/components/SiteNav";
import { repoUrl } from "@/lib/site";

describe("SiteNav", () => {
  it("links the project repository in the desktop bar and the mobile panel", async () => {
    const user = userEvent.setup();
    render(<SiteNav />);

    const desktopLink = screen.getByRole("link", { name: "View source on GitHub" });
    expect(desktopLink).toHaveAttribute("href", repoUrl);
    expect(desktopLink).toHaveAttribute("target", "_blank");
    expect(desktopLink).toHaveAttribute("rel", "noopener noreferrer");

    await user.click(screen.getByRole("button", { name: "Open navigation menu" }));

    const mobileNav = screen.getByRole("navigation", { name: "Mobile" });
    const mobileLink = within(mobileNav).getByRole("link", { name: "View source on GitHub" });
    expect(mobileLink).toHaveAttribute("href", repoUrl);
    expect(mobileLink).toHaveAttribute("target", "_blank");
    expect(mobileLink).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("opens the mobile disclosure, focuses a link, and closes on Escape", async () => {
    const user = userEvent.setup();
    render(<SiteNav />);

    const menuButton = screen.getByRole("button", { name: "Open navigation menu" });
    await user.click(menuButton);

    expect(screen.getByRole("button", { name: "Close navigation menu" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    const mobileNav = screen.getByRole("navigation", { name: "Mobile" });
    expect(mobileNav).toBeInTheDocument();
    expect(within(mobileNav).getByRole("link", { name: "Signal path" })).toHaveFocus();

    await user.keyboard("{Escape}");

    expect(screen.getByRole("button", { name: "Open navigation menu" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });
});
