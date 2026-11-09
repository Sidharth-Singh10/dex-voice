import { ArrowUp, ArrowUpRight, GithubLogo } from "@phosphor-icons/react/dist/ssr";
import { BrandMark } from "@/components/BrandMark";
import { navLinks } from "@/lib/content";
import { repoLabel, repoUrl } from "@/lib/site";

const waveform = [
  16, 28, 42, 22, 54, 34, 70, 44, 26, 62, 38, 88, 52, 30, 74, 46, 24, 58, 34, 82, 48, 28, 66,
  38, 92, 56, 32, 70, 44, 22, 60, 36, 78, 50, 28, 64, 40, 20, 56, 34, 86, 48, 26, 68, 42, 18,
  52, 30, 72, 44, 24, 58, 36, 80, 46, 28, 62, 38, 20, 50,
];

export function FooterCta() {
  return (
    <>
      <section aria-labelledby="action-title" className="border-t border-line">
        <div className="container-shell section-pad">
          <div className="panel-raised relative overflow-hidden px-5 py-10 sm:p-10 md:p-14 lg:p-16">
            <div
              className="pointer-events-none absolute -right-24 top-0 h-80 w-80 rounded-full bg-accent/[0.08] blur-3xl"
              aria-hidden="true"
            />
            <div className="relative max-w-[54rem]">
              <h2
                id="action-title"
                className="text-[clamp(2.4rem,6.5vw,5.8rem)] font-extrabold leading-[0.9] tracking-[-0.062em]"
              >
                The voice is local. The{" "}
                <span className="text-accent">caveats</span> are too.
              </h2>
              <p className="mt-6 max-w-[46ch] text-[1rem] leading-[1.65] text-muted">
                Read the setup or the limits. Both are part of the same machine, and neither one
                is hidden behind a download button.
              </p>
              <div className="mt-8 flex flex-col gap-3 sm:flex-row">
                <a href="#setup" className="btn btn-primary w-full sm:w-auto">
                  Set up on Linux
                  <ArrowUpRight size={17} weight="bold" aria-hidden="true" />
                </a>
                <a href="#limits" className="btn btn-secondary w-full sm:w-auto">
                  Review the limits
                  <ArrowUp size={16} weight="bold" aria-hidden="true" />
                </a>
              </div>
            </div>

            <div className="relative mt-12 overflow-hidden border-t border-line pt-7" aria-hidden="true">
              <div className="flex h-20 items-end gap-[2px] opacity-75 sm:gap-[4px]">
                {waveform.map((height, index) => (
                  <span
                    key={`${height}-${index}`}
                    className={`w-full rounded-t-[1px] ${
                      index % 13 === 0 ? "bg-accent/80" : "bg-bone/20"
                    }`}
                    style={{ height: `${height}%` }}
                  />
                ))}
              </div>
              <div className="mt-3 flex items-center justify-between gap-2 whitespace-nowrap font-mono text-[8px] uppercase tracking-[0.12em] text-faint">
                <span>physical mic</span>
                <span>dex_voice</span>
                <span className="hidden sm:inline">dex_mic / DexMicrophone</span>
                <span className="sm:hidden">dex_mic</span>
              </div>
            </div>
          </div>
        </div>
      </section>

      <footer className="border-t border-line">
        <div className="container-shell py-10 sm:py-12">
          <div className="flex flex-col gap-9 lg:flex-row lg:items-start lg:justify-between">
            <div className="max-w-sm">
              <a href="#top" className="inline-flex items-center gap-2.5" aria-label="Dex Voice home">
                <BrandMark className="text-bone" />
                <span className="text-[1.05rem] font-extrabold tracking-[-0.035em]">Dex Voice</span>
              </a>
              <p className="mt-4 text-[0.86rem] leading-[1.6] text-muted">
                A Linux-first experimental local voice changer. Still a build, not a product
                launch.
              </p>
            </div>

            <nav className="flex flex-wrap gap-x-6 gap-y-3" aria-label="Footer">
              {navLinks.map((link) => (
                <a
                  key={link.href}
                  href={link.href}
                  className="text-[0.85rem] text-muted transition-colors hover:text-bone"
                >
                  {link.label}
                </a>
              ))}
              <a
                href="#status"
                className="text-[0.85rem] text-muted transition-colors hover:text-bone"
              >
                Status
              </a>
              <a
                href={repoUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-[0.85rem] text-muted transition-colors hover:text-bone"
              >
                GitHub
                <ArrowUpRight size={12} weight="bold" aria-hidden="true" />
              </a>
            </nav>

            <div className="lg:text-right">
              <p className="font-mono text-[9px] uppercase tracking-[0.1em] text-accent">
                Linux experimental
              </p>
              <p className="mt-2 font-mono text-[9px] uppercase tracking-[0.08em] text-faint">
                No public build / no account
              </p>
              <a
                href={repoUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="mt-3 inline-flex items-center gap-2 font-mono text-[10px] tracking-[0.02em] text-muted transition-colors hover:text-bone"
              >
                <GithubLogo size={13} weight="bold" aria-hidden="true" />
                {repoLabel}
              </a>
            </div>
          </div>

          <div className="mt-10 flex flex-col gap-3 border-t border-line pt-5 text-[0.76rem] leading-relaxed text-faint sm:flex-row sm:items-center sm:justify-between">
            <p>
              © {new Date().getFullYear()} Dex Voice. Built with Parakeet, transcribe.cpp, Phonon,
              PipeWire, and Rune.
            </p>
            <p className="font-mono text-[9px] uppercase tracking-[0.08em]">
              No cloud. No account. No telemetry.
            </p>
          </div>
        </div>
      </footer>
    </>
  );
}
