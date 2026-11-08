import { Check, Info, WarningCircle } from "@phosphor-icons/react/dist/ssr";
import { blockedItems, workingItems } from "@/lib/content";

export function StatusSection() {
  return (
    <section id="status" aria-labelledby="status-title" className="section-pad border-t border-line">
      <div className="container-shell">
        <div className="max-w-4xl">
          <h2
            id="status-title"
            className="text-[clamp(2.2rem,5.6vw,4.8rem)] font-extrabold leading-[0.95] tracking-[-0.055em]"
          >
            Where it stands.
          </h2>
          <p className="mt-6 max-w-[56ch] text-[1rem] leading-[1.65] text-muted md:text-[1.08rem]">
            Linux is the only tested path. Several pieces work end to end. The important ones
            still do not. This is the honest split.
          </p>
        </div>

        <div className="mt-14 grid grid-cols-1 gap-3 md:mt-20 lg:grid-cols-2">
          <div className="panel overflow-hidden">
            <div className="flex items-center gap-2 border-b border-line px-5 py-4 sm:px-7">
              <span className="flex h-6 w-6 items-center justify-center rounded-full border border-line text-accent">
                <Check size={13} weight="bold" aria-hidden="true" />
              </span>
              <h3 className="text-[0.95rem] font-bold tracking-[-0.02em] text-bone">
                Working on this laptop
              </h3>
            </div>
            <ul className="divide-y divide-line">
              {workingItems.map((item) => (
                <li key={item.title} className="px-5 py-5 sm:px-7">
                  <div className="flex items-start gap-3">
                    <Check
                      size={15}
                      weight="bold"
                      className="mt-0.5 shrink-0 text-accent"
                      aria-hidden="true"
                    />
                    <div>
                      <h4 className="text-[0.92rem] font-bold tracking-[-0.015em] text-bone">
                        {item.title}
                      </h4>
                      <p className="mt-1.5 max-w-[46ch] text-[0.85rem] leading-[1.6] text-muted">
                        {item.detail}
                      </p>
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          </div>

          <div className="panel overflow-hidden">
            <div className="flex items-center gap-2 border-b border-line px-5 py-4 sm:px-7">
              <span className="flex h-6 w-6 items-center justify-center rounded-full border border-accent/40 bg-accent/[0.06] text-accent">
                <WarningCircle size={13} weight="bold" aria-hidden="true" />
              </span>
              <h3 className="text-[0.95rem] font-bold tracking-[-0.02em] text-bone">
                Not ready or unverified
              </h3>
            </div>
            <ul className="divide-y divide-line">
              {blockedItems.map((item) => {
                const verifyOnly = item.severity === "verify";
                const Icon = verifyOnly ? Info : WarningCircle;
                return (
                  <li key={item.title} className="px-5 py-5 sm:px-7">
                    <div className="flex items-start gap-3">
                      <Icon
                        size={15}
                        weight="bold"
                        className={`mt-0.5 shrink-0 ${verifyOnly ? "text-muted" : "text-accent"}`}
                        aria-hidden="true"
                      />
                      <div>
                        <h4 className="text-[0.92rem] font-bold tracking-[-0.015em] text-bone">
                          {item.title}
                        </h4>
                        <p className="mt-1.5 max-w-[46ch] text-[0.85rem] leading-[1.6] text-muted">
                          {item.detail}
                        </p>
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          </div>
        </div>

        <div className="mt-3 flex flex-col gap-2 border-t border-line pt-5 text-[0.8rem] leading-relaxed text-faint sm:flex-row sm:items-center sm:justify-between">
          <p>Status recorded on the development laptop. This is not a support policy.</p>
          <p className="font-mono text-[9px] uppercase tracking-[0.1em] text-muted">
            Linux only / no public build
          </p>
        </div>
      </div>
    </section>
  );
}
