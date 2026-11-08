import { Check } from "@phosphor-icons/react/dist/ssr";
import { voices } from "@/lib/content";

function voiceBars(seed: number) {
  return Array.from({ length: 44 }, (_, index) => {
    const primary = Math.abs(Math.sin((index + seed * 7) / 3.9));
    const secondary = Math.abs(Math.sin((index + seed * 3) / 1.35));
    return Math.round(18 + primary * 64 + secondary * 14);
  });
}

export function VoicesSection() {
  return (
    <section id="voices" aria-labelledby="voices-title" className="section-pad border-t border-line">
      <div className="container-shell">
        <div className="grid grid-cols-1 gap-12 lg:grid-cols-[0.78fr_1.22fr] lg:gap-16">
          <div className="lg:sticky lg:top-28 lg:self-start">
            <h2
              id="voices-title"
              className="max-w-[10ch] text-[clamp(2.5rem,6vw,5.25rem)] font-extrabold leading-[0.9] tracking-[-0.06em]"
            >
              Three voices. One local model.
            </h2>
            <p className="mt-6 max-w-[36ch] text-[1rem] leading-[1.65] text-muted">
              Voice selection is independent of the recognizer. All three embeddings run through
              the same Phonon checkpoint on the CPU.
            </p>

            <ul className="mt-8 space-y-3 border-t border-line pt-5">
              {[
                "Default, Zoey, and Juniper",
                "Installed by the Linux model script",
                "No voice sample is embedded on this page",
              ].map((item) => (
                <li key={item} className="flex items-start gap-2.5 text-[0.88rem] text-bone/80">
                  <Check
                    size={14}
                    weight="bold"
                    className="mt-0.5 shrink-0 text-accent"
                    aria-hidden="true"
                  />
                  {item}
                </li>
              ))}
            </ul>

            <p className="mt-8 max-w-[34ch] border-l border-accent/40 pl-4 text-[0.84rem] leading-[1.6] text-faint">
              Changing the voice does not make latency faster. It is a different output
              embedding, not a shortcut through the queue.
            </p>
          </div>

          <div className="space-y-3">
            {voices.map((voice, index) => (
              <article
                key={voice.name}
                className="group relative overflow-hidden rounded-[var(--radius-panel)] border border-line bg-panel p-5 transition-colors duration-500 hover:border-accent/40 sm:p-7"
              >
                <span
                  className="absolute left-0 top-0 h-[2px] w-0 bg-accent transition-[width] duration-700 group-hover:w-full"
                  aria-hidden="true"
                />
                <div className="relative flex flex-wrap items-start justify-between gap-4">
                  <div>
                    <p className="font-mono text-[9px] uppercase tracking-[0.12em] text-faint">
                      Voice 0{index + 1}
                    </p>
                    <h3 className="mt-3 text-[clamp(2rem,4vw,3.2rem)] font-extrabold leading-none tracking-[-0.055em] text-bone">
                      {voice.name}
                    </h3>
                  </div>
                  <span className="inline-flex items-center gap-1.5 rounded-full border border-line px-2.5 py-1 font-mono text-[9px] uppercase tracking-[0.09em] text-muted">
                    <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
                    installed
                  </span>
                </div>

                <p className="relative mt-3 font-mono text-[9px] uppercase tracking-[0.08em] text-faint">
                  {voice.meta}
                </p>
                <p className="relative mt-2 max-w-[52ch] text-[0.9rem] leading-[1.6] text-muted">
                  {voice.description}
                </p>

                <div className="relative mt-7 overflow-hidden">
                  <div
                    className="flex h-24 items-end gap-[3px] opacity-80 transition-transform duration-700 ease-out group-hover:translate-x-2"
                    aria-hidden="true"
                  >
                    {voiceBars(index + 1).map((height, barIndex) => (
                      <span
                        key={`${voice.name}-${barIndex}`}
                        className={`w-full rounded-t-[2px] ${
                          barIndex % 11 === 0 ? "bg-accent" : "bg-bone/25"
                        }`}
                        style={{ height: `${height}%` }}
                      />
                    ))}
                  </div>
                  <div className="mt-2 flex items-center justify-between border-t border-line pt-2.5 font-mono text-[8px] uppercase tracking-[0.1em] text-faint">
                    <span>{voice.stamp}</span>
                    <span>illustrative, not a sample</span>
                  </div>
                </div>
              </article>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
