import { truthFacts } from "@/lib/content";

export function TruthStrip() {
  return (
    <section aria-label="Project facts" className="relative z-10 pb-2">
      <div className="container-shell">
        <div className="grid grid-cols-2 gap-px overflow-hidden rounded-[var(--radius-panel)] border border-line bg-line lg:grid-cols-4">
          {truthFacts.map((fact) => (
            <div key={fact.label} className="bg-ink px-4 py-5 sm:px-6 sm:py-6">
              <div className="flex items-center gap-2">
                <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
                <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-bone">
                  {fact.label}
                </p>
              </div>
              <p className="mt-2 max-w-[28ch] text-[0.83rem] leading-[1.5] text-muted">
                {fact.detail}
              </p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
