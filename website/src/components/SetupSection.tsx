import { Check, Info, TerminalWindow } from "@phosphor-icons/react/dist/ssr";
import { CopyButton } from "@/components/CopyButton";
import {
  prerequisites,
  setupSteps,
  shutdownCleanup,
  troubleshootingCommands,
} from "@/lib/content";

export function SetupSection() {
  return (
    <section id="setup" aria-labelledby="setup-title" className="section-pad border-t border-line">
      <div className="container-shell">
        <div className="max-w-4xl">
          <h2
            id="setup-title"
            className="text-[clamp(2.2rem,5.6vw,4.8rem)] font-extrabold leading-[0.95] tracking-[-0.055em]"
          >
            Set it up on Linux.
          </h2>
          <p className="mt-6 max-w-[58ch] text-[1rem] leading-[1.65] text-muted md:text-[1.08rem]">
            This page is a showcase, not a download page. There is no public build or installer.
            Start from a local checkout of the repository and follow the route below.
          </p>
        </div>

        <div className="mt-14 grid grid-cols-1 gap-10 lg:mt-20 lg:grid-cols-[0.72fr_1.28fr] lg:gap-16">
          <aside className="min-w-0 lg:sticky lg:top-28 lg:self-start">
            <div className="panel p-5 sm:p-6">
              <div className="flex items-center gap-2">
                <TerminalWindow size={16} weight="bold" className="text-accent" aria-hidden="true" />
                <h3 className="text-[0.95rem] font-bold tracking-[-0.02em] text-bone">
                  Before you start
                </h3>
              </div>
              <ul className="mt-4 space-y-2.5">
                {prerequisites.map((item) => (
                  <li key={item} className="flex items-start gap-2.5 text-[0.84rem] leading-[1.5] text-muted">
                    <Check
                      size={13}
                      weight="bold"
                      className="mt-0.5 shrink-0 text-accent"
                      aria-hidden="true"
                    />
                    {item}
                  </li>
                ))}
              </ul>
              <div className="mt-5 border-t border-line pt-4">
                <p className="text-[0.8rem] leading-[1.55] text-faint">
                  Full build flags, vendored Vulkan headers, and distro notes live in{" "}
                  <span className="font-mono text-[0.75rem] text-muted">rust/README.md</span>. The
                  route is documented in{" "}
                  <span className="font-mono text-[0.75rem] text-muted">rust/LINUX.md</span>.
                </p>
              </div>
            </div>

            <div className="mt-3 rounded-[var(--radius-panel)] border border-line bg-panel/55 p-5">
              <div className="flex items-center gap-2 text-muted">
                <Info size={15} weight="bold" aria-hidden="true" />
                <p className="font-mono text-[9px] uppercase tracking-[0.1em]">
                  {shutdownCleanup.title}
                </p>
              </div>
              <p className="mt-3 text-[0.84rem] leading-[1.6] text-muted">
                {shutdownCleanup.detail}
              </p>
            </div>
          </aside>

          <div className="min-w-0">
            <ol className="space-y-2.5">
              {setupSteps.map((step, index) => (
                <li key={step.id} className="panel p-5 sm:p-6">
                  <div className="flex items-start gap-4">
                    <span className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-line font-mono text-[10px] text-accent">
                      {String(index + 1).padStart(2, "0")}
                    </span>
                    <div className="min-w-0 flex-1">
                      <h3 className="text-[1rem] font-bold tracking-[-0.025em] text-bone">
                        {step.title}
                      </h3>
                      <p className="mt-2 max-w-[56ch] text-[0.86rem] leading-[1.6] text-muted">
                        {step.detail}
                      </p>

                      {step.command ? (
                        <div className="mt-4 overflow-hidden rounded-lg border border-line bg-ink-deep">
                          <div className="flex items-center justify-between gap-3 border-b border-line px-3 py-1.5">
                            <span className="font-mono text-[8px] uppercase tracking-[0.12em] text-faint">
                              shell
                            </span>
                            <CopyButton value={step.command} label="Copy" />
                          </div>
                          <div className="flex min-w-0 items-start gap-2 px-3 py-2.5">
                            <span
                              className="shrink-0 font-mono text-[11px] leading-[1.7] text-accent"
                              aria-hidden="true"
                            >
                              $
                            </span>
                            <code className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-[11px] leading-[1.7] text-bone/90">
                              {step.command}
                            </code>
                          </div>
                        </div>
                      ) : (
                        <div className="mt-4 inline-flex items-center gap-2 rounded-lg border border-accent/30 bg-accent/[0.06] px-3 py-2 font-mono text-[10px] uppercase tracking-[0.06em] text-accent">
                          <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
                          Select DexMicrophone
                        </div>
                      )}
                    </div>
                  </div>
                </li>
              ))}
            </ol>

            <div className="mt-3 panel overflow-hidden">
              <div className="flex items-center gap-2 border-b border-line px-5 py-4 sm:px-6">
                <TerminalWindow size={15} weight="bold" className="text-accent" aria-hidden="true" />
                <h3 className="text-[0.9rem] font-bold tracking-[-0.02em] text-bone">
                  If the route misbehaves
                </h3>
              </div>
              <div className="divide-y divide-line">
                {troubleshootingCommands.map((item) => (
                  <div key={item.command} className="px-5 py-4 sm:px-6">
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-[0.82rem] leading-[1.5] text-muted">{item.label}</p>
                      <CopyButton value={item.command} label="Copy" />
                    </div>
                    <div className="mt-2 flex min-w-0 items-start gap-2 rounded-lg border border-line bg-ink-deep px-3 py-2.5">
                      <span className="shrink-0 font-mono text-[11px] leading-[1.7] text-accent" aria-hidden="true">
                        $
                      </span>
                      <code className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-[10.5px] leading-[1.7] text-bone/90">
                        {item.command}
                      </code>
                    </div>
                    {"note" in item && item.note ? (
                      <p className="mt-2 font-mono text-[9px] leading-relaxed tracking-[0.02em] text-faint">
                        {item.note}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
            </div>

            <p className="mt-5 max-w-[58ch] text-[0.82rem] leading-[1.6] text-faint">
              The setup script does not change your normal speaker or microphone. To undo it, list
              the Dex modules and unload each module ID. If a calling app does not list
              DexMicrophone, reopen or reload its microphone device list, then reselect the source.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
