"use client";

import { useEffect, useRef, useState } from "react";
import { ArrowUpRight, GithubLogo, List, X } from "@phosphor-icons/react";
import { navLinks } from "@/lib/content";
import { repoUrl } from "@/lib/site";
import { BrandMark } from "@/components/BrandMark";

export function SiteNav() {
  const [open, setOpen] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;

    const firstLink = panelRef.current?.querySelector<HTMLAnchorElement>("a");
    firstLink?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        buttonRef.current?.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open]);

  return (
    <header className="fixed inset-x-0 top-0 z-50">
      <div className="container-shell">
        <div className="relative mt-2 flex h-16 items-center justify-between rounded-[14px] border border-line bg-ink/78 px-3 shadow-[0_12px_40px_rgba(0,0,0,0.18)] backdrop-blur-xl sm:px-4">
          <a
            href="#top"
            className="group flex items-center gap-2.5 rounded-lg py-2 pl-1 pr-2"
            aria-label="Dex Voice home"
          >
            <BrandMark className="text-bone transition-transform duration-300 group-hover:scale-[1.06]" />
            <span className="flex items-baseline gap-2">
              <span className="text-[1.05rem] font-extrabold leading-none tracking-[-0.035em]">
                Dex Voice
              </span>
              <span className="hidden font-mono text-[9px] uppercase tracking-[0.14em] text-muted sm:inline">
                Linux
              </span>
            </span>
          </a>

          <nav className="hidden items-center gap-1 lg:flex" aria-label="Primary">
            {navLinks.map((link) => (
              <a
                key={link.href}
                href={link.href}
                className="rounded-lg px-3 py-2 text-[0.84rem] font-medium text-muted transition-colors hover:bg-bone/5 hover:text-bone"
              >
                {link.label}
              </a>
            ))}
          </nav>

          <div className="flex items-center gap-2">
            <a
              href={repoUrl}
              target="_blank"
              rel="noopener noreferrer"
              aria-label="View source on GitHub"
              className="hidden h-9 w-9 items-center justify-center rounded-lg border border-line text-muted transition-colors hover:bg-bone/5 hover:text-bone sm:flex"
            >
              <GithubLogo size={17} weight="bold" aria-hidden="true" />
            </a>
            <span className="hidden items-center gap-2 rounded-full border border-line px-3 py-1.5 font-mono text-[9px] uppercase tracking-[0.12em] text-muted xl:inline-flex">
              <span className="soft-pulse h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
              Experimental build
            </span>
            <a href="#setup" className="btn btn-primary btn-sm hidden sm:inline-flex">
              Set up on Linux
              <ArrowUpRight size={15} weight="bold" aria-hidden="true" />
            </a>
            <button
              ref={buttonRef}
              type="button"
              className="flex h-11 w-11 items-center justify-center rounded-lg border border-line text-bone transition-colors hover:bg-bone/5 lg:hidden"
              aria-expanded={open}
              aria-controls="mobile-nav"
              aria-label={open ? "Close navigation menu" : "Open navigation menu"}
              onClick={() => setOpen((current) => !current)}
            >
              {open ? <X size={19} weight="bold" /> : <List size={19} weight="bold" />}
            </button>
          </div>

          <div
            id="mobile-nav"
            ref={panelRef}
            className={`absolute left-0 right-0 top-[calc(100%+8px)] overflow-hidden rounded-[14px] border border-line bg-panel shadow-[0_24px_70px_rgba(0,0,0,0.45)] backdrop-blur-xl transition-[opacity,transform] duration-200 lg:hidden ${
              open
                ? "pointer-events-auto translate-y-0 opacity-100"
                : "pointer-events-none -translate-y-1 opacity-0"
            }`}
            aria-hidden={!open}
            inert={!open}
          >
            <nav className="flex flex-col p-2" aria-label="Mobile">
              {navLinks.map((link) => (
                <a
                  key={link.href}
                  href={link.href}
                  onClick={() => setOpen(false)}
                  className="rounded-lg px-3 py-3 text-base font-medium text-bone transition-colors hover:bg-bone/5"
                >
                  {link.label}
                </a>
              ))}
              <a
                href={repoUrl}
                target="_blank"
                rel="noopener noreferrer"
                onClick={() => setOpen(false)}
                className="mt-2 flex items-center justify-between rounded-lg border border-line px-3 py-3 text-base font-medium text-bone transition-colors hover:bg-bone/5"
              >
                <span className="flex items-center gap-2">
                  <GithubLogo size={17} weight="bold" aria-hidden="true" />
                  View source on GitHub
                </span>
                <ArrowUpRight size={16} weight="bold" aria-hidden="true" />
              </a>
              <a
                href="#setup"
                onClick={() => setOpen(false)}
                className="btn btn-primary mt-2 w-full"
              >
                Set up on Linux
                <ArrowUpRight size={16} weight="bold" aria-hidden="true" />
              </a>
              <p className="px-3 pb-2 pt-3 font-mono text-[10px] uppercase tracking-[0.12em] text-muted">
                Linux only / no public build
              </p>
            </nav>
          </div>
        </div>
      </div>
    </header>
  );
}
