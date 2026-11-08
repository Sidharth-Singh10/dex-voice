import { Info, WarningCircle } from "@phosphor-icons/react/dist/ssr";
import { envelopeFacts, resynthesisNote } from "@/lib/content";

export function EnvelopeSection() {
  return (
    <section aria-labelledby="envelope-title" className="section-pad border-t border-line">
      <div className="container-shell">
        <div className="max-w-4xl">
          <h2
            id="envelope-title"
            className="text-[clamp(2.2rem,5.6vw,4.8rem)] font-extrabold leading-[0.95] tracking-[-0.055em]"
          >
            The operating envelope.
          </h2>
          <p className="mt-6 max-w-[56ch] text-[1rem] leading-[1.65] text-muted md:text-[1.08rem]">
            These are behavior settings, not benchmark claims. They explain why Dex sounds
            deliberate in a call, and why it is not a real-time changer.
          </p>
        </div>

        <div className="mt-14 grid gap-px overflow-hidden rounded-[var(--radius-panel)] border border-line bg-line md:mt-20 md:grid-cols-3">
          {envelopeFacts.map((fact, index) => (
            <div key={fact.value} className="bg-ink p-5 sm:p-7">
              <div className="flex items-baseline justify-between gap-4">
                <p className="mono-value text-[clamp(2.7rem,6vw,5rem)] font-bold leading-none text-bone">
                  {fact.value}
                </p>
                <span className="font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
                  0{index + 1}
                </span>
              </div>
              <p className="mt-4 max-w-[22ch] text-[0.95rem] font-bold leading-[1.25] tracking-[-0.02em] text-bone">
                {fact.label}
              </p>
              <p className="mt-3 max-w-[34ch] text-[0.85rem] leading-[1.6] text-muted">
                {fact.detail}
              </p>
            </div>
          ))}
        </div>

        <div className="mt-3 flex items-start gap-3 rounded-[var(--radius-panel)] border border-line bg-panel/45 px-4 py-3.5 sm:px-5">
          <Info size={16} weight="bold" className="mt-0.5 shrink-0 text-muted" aria-hidden="true" />
          <p className="max-w-[72ch] text-[0.86rem] leading-[1.6] text-muted">
            <span className="font-bold text-bone">{resynthesisNote.lead}</span>{" "}
            {resynthesisNote.detail}
          </p>
        </div>

        <div className="mt-3 grid grid-cols-1 gap-3 lg:grid-cols-[1.12fr_0.88fr]">
          <figure className="panel overflow-hidden p-5 sm:p-7">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <p className="mono-label">Half-duplex, visualised</p>
                <h3 className="mt-2 text-[1.4rem] font-extrabold tracking-[-0.035em] text-bone">
                  Overlap has nowhere to go.
                </h3>
              </div>
              <span className="rounded-full border border-line px-2.5 py-1 font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
                diagram
              </span>
            </div>

            <div className="mt-7 overflow-hidden rounded-xl border border-line bg-ink p-3.5 sm:p-4">
              <div className="grid grid-cols-[44px_1fr] gap-x-3 gap-y-2.5">
                <span className="self-center font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
                  You
                </span>
                <div className="grid grid-cols-[1.55fr_0.7fr_1.1fr] gap-1">
                  <div className="rounded-md border border-bone/15 bg-bone/[0.06] px-2 py-2 text-[10px] leading-tight text-bone/80">
                    phrase
                  </div>
                  <div className="rounded-md border border-dashed border-line px-1.5 py-2 text-center font-mono text-[8px] uppercase leading-tight tracking-[0.04em] text-faint">
                    800 ms
                    <br />
                    quiet
                  </div>
                  <div className="relative overflow-hidden rounded-md border border-dashed border-accent/40 bg-accent/[0.06] px-2 py-2 text-[10px] leading-tight text-accent">
                    speech lost
                    <span
                      className="absolute inset-0 opacity-25"
                      style={{
                        backgroundImage:
                          "repeating-linear-gradient(135deg, transparent 0 6px, rgba(234,122,76,.45) 6px 8px)",
                      }}
                      aria-hidden="true"
                    />
                  </div>
                </div>

                <span className="self-center font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
                  Dex
                </span>
                <div className="grid grid-cols-[1.55fr_0.7fr_1.1fr] gap-1">
                  <div className="rounded-md border border-transparent px-2 py-2 text-[10px] text-faint">
                    listening
                  </div>
                  <div className="rounded-md border border-transparent px-1.5 py-2 text-center font-mono text-[8px] uppercase leading-tight tracking-[0.04em] text-faint">
                    endpoint
                  </div>
                  <div className="rounded-md border border-accent/40 bg-accent/15 px-2 py-2 text-[10px] font-medium leading-tight text-bone">
                    speaking
                    <span className="mt-0.5 block font-mono text-[8px] uppercase tracking-[0.05em] text-accent">
                      mic muted
                    </span>
                  </div>
                </div>
              </div>
              <div className="mt-3 flex items-center gap-2 border-t border-line pt-2.5 font-mono text-[8px] uppercase tracking-[0.08em] text-faint">
                <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
                Phrase boundary
                <span className="ml-auto">No echo cancellation</span>
              </div>
            </div>

            <figcaption className="mt-5 max-w-[52ch] text-[0.86rem] leading-[1.6] text-muted">
              Dex waits for the pause, then speaks. While it is speaking it is not listening, so
              anything you say over it is discarded. Use headphones if another app plays
              DexMicrophone on your speakers.
            </figcaption>
          </figure>

          <div className="panel relative flex flex-col justify-between overflow-hidden p-5 sm:p-7">
            <div
              className="pointer-events-none absolute -right-16 -top-16 h-48 w-48 rounded-full bg-accent/10 blur-3xl"
              aria-hidden="true"
            />
            <div className="relative">
              <div className="flex items-center gap-2 text-accent">
                <WarningCircle size={18} weight="bold" aria-hidden="true" />
                <p className="font-mono text-[10px] uppercase tracking-[0.12em]">
                  Target not met
                </p>
              </div>
              <h3 className="mt-5 text-[clamp(2rem,4.4vw,3.5rem)] font-extrabold leading-[0.94] tracking-[-0.055em]">
                Under 500 ms is still out of reach.
              </h3>
              <p className="mt-5 max-w-[40ch] text-[0.93rem] leading-[1.65] text-muted">
                This is not a real-time voice changer today. The phrase wait and the synthesis
                queue put actual use well beyond the latency target. Optimization is deferred,
                not quietly redefined.
              </p>
            </div>
            <div className="relative mt-8 border-t border-line pt-4">
              <p className="max-w-[38ch] text-[0.82rem] leading-[1.55] text-faint">
                The app reports a group-commit metric, not acoustic end-to-end latency. Treat
                any number on screen as a diagnostic, not a promise.
              </p>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
