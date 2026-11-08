"use client";

import { useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import { ArrowLeft, ArrowRight, Check } from "@phosphor-icons/react";
import { pipelineStages, type StageId } from "@/lib/content";

const micBars = [20, 42, 28, 66, 38, 84, 54, 30, 74, 46, 92, 58, 34, 62, 26, 48, 18];

function StageVisual({ id }: { id: StageId }) {
  if (id === "mic") {
    return (
      <div className="relative flex h-full min-h-[276px] flex-col justify-between overflow-hidden rounded-xl border border-line bg-ink p-5">
        <div className="flex items-center justify-between font-mono text-[9px] uppercase tracking-[0.12em] text-faint">
          <span>capture / cpal</span>
          <span>physical input</span>
        </div>
        <div className="relative flex h-[150px] items-center justify-center">
          <div className="absolute h-36 w-36 rounded-full border border-bone/[0.06]" aria-hidden="true" />
          <div className="absolute h-24 w-24 rounded-full border border-accent/15" aria-hidden="true" />
          <div className="relative flex items-center gap-1.5" aria-hidden="true">
            {micBars.map((height, index) => (
              <span
                key={`${height}-${index}`}
                className={`w-[4px] rounded-full ${index === 9 ? "bg-accent" : "bg-bone/40"}`}
                style={{ height: `${height}px` }}
              />
            ))}
          </div>
        </div>
        <p className="max-w-[24ch] font-mono text-[10px] leading-relaxed tracking-[0.02em] text-muted">
          No system defaults are changed. Your selected device is opened directly.
        </p>
      </div>
    );
  }

  if (id === "asr") {
    return (
      <div className="relative flex h-full min-h-[276px] flex-col overflow-hidden rounded-xl border border-line bg-ink p-5">
        <div className="flex items-center justify-between font-mono text-[9px] uppercase tracking-[0.12em] text-faint">
          <span>transcribe.cpp</span>
          <span>vulkan</span>
        </div>
        <div className="relative my-auto flex items-center justify-center py-5">
          <div
            className="absolute inset-x-12 top-1/2 h-px bg-gradient-to-r from-transparent via-accent/25 to-transparent"
            aria-hidden="true"
          />
          <div className="relative rounded-xl border border-accent/35 bg-panel-raised px-7 py-5 shadow-[0_0_40px_rgba(234,122,76,0.07)]">
            <span className="absolute -left-1 top-1/2 h-2 w-2 -translate-y-1/2 rounded-full bg-accent" aria-hidden="true" />
            <span className="absolute -right-1 top-1/2 h-2 w-2 -translate-y-1/2 rounded-full bg-accent" aria-hidden="true" />
            <p className="mono-value text-center text-5xl font-bold leading-none text-bone">F16</p>
            <p className="mt-2 text-center font-mono text-[9px] uppercase tracking-[0.16em] text-accent">
              GGUF / NVIDIA
            </p>
          </div>
        </div>
        <div className="flex items-center justify-between gap-4 font-mono text-[9px] uppercase tracking-[0.08em] text-faint">
          <span>live word commits</span>
          <span>Vulkan, selected explicitly</span>
        </div>
      </div>
    );
  }

  if (id === "tts") {
    return (
      <div className="relative flex h-full min-h-[276px] flex-col overflow-hidden rounded-xl border border-line bg-ink p-5">
        <div className="flex items-center justify-between font-mono text-[9px] uppercase tracking-[0.12em] text-faint">
          <span>phonon 31m</span>
          <span>cpu synthesis</span>
        </div>
        <div className="relative my-auto flex h-[150px] items-center justify-center" aria-hidden="true">
          <div className="absolute h-36 w-36 rounded-full border border-bone/[0.07]" />
          <div className="absolute h-24 w-24 rounded-full border border-bone/[0.12]" />
          <div className="absolute h-12 w-12 rounded-full border border-accent/40 bg-accent/10" />
          <span className="absolute left-1 top-4 h-2 w-2 rounded-full bg-accent" />
          <span className="absolute right-3 top-10 h-1.5 w-1.5 rounded-full bg-bone/40" />
          <span className="absolute bottom-3 left-12 h-1.5 w-1.5 rounded-full bg-bone/40" />
          <span className="relative z-10 font-mono text-[9px] uppercase tracking-[0.14em] text-accent">
            voice
          </span>
        </div>
        <div className="flex items-center justify-between gap-4 font-mono text-[9px] uppercase tracking-[0.08em] text-faint">
          <span>default</span>
          <span>zoey</span>
          <span>juniper</span>
        </div>
      </div>
    );
  }

  return (
    <div className="relative flex h-full min-h-[276px] flex-col overflow-hidden rounded-xl border border-line bg-ink p-5">
      <div className="flex items-center justify-between font-mono text-[9px] uppercase tracking-[0.12em] text-faint">
        <span>pipewire / pulse</span>
        <span>virtual route</span>
      </div>
      <div className="my-auto py-4">
        <div className="mx-auto max-w-[270px]">
          <div className="rounded-lg border border-line bg-panel px-3.5 py-3">
            <div className="flex items-center justify-between gap-3">
              <span className="font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
                dex_voice
              </span>
              <span className="rounded-full border border-line px-2 py-0.5 font-mono text-[8px] uppercase tracking-[0.1em] text-muted">
                sink
              </span>
            </div>
            <p className="mt-1 text-sm font-bold tracking-[-0.02em] text-bone">App output</p>
          </div>
          <div className="relative mx-auto h-9 w-px bg-accent/40" aria-hidden="true">
            <span className="absolute -bottom-0.5 left-1/2 h-1.5 w-1.5 -translate-x-1/2 rotate-45 border-b border-r border-accent" />
          </div>
          <div className="rounded-lg border border-accent/40 bg-accent/[0.07] px-3.5 py-3">
            <div className="flex items-center justify-between gap-3">
              <span className="font-mono text-[9px] uppercase tracking-[0.1em] text-accent">
                dex_mic
              </span>
              <span className="rounded-full border border-accent/30 px-2 py-0.5 font-mono text-[8px] uppercase tracking-[0.1em] text-accent">
                source
              </span>
            </div>
            <p className="mt-1 text-sm font-bold tracking-[-0.02em] text-bone">DexMicrophone</p>
          </div>
        </div>
      </div>
      <div className="flex items-center justify-between gap-4 font-mono text-[9px] uppercase tracking-[0.08em] text-faint">
        <span>remapped source</span>
        <span>verify app listing</span>
      </div>
    </div>
  );
}

export function PipelineSection() {
  const [active, setActive] = useState(0);
  const root = useRef<HTMLElement>(null);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const stage = pipelineStages[active];

  const columns = pipelineStages
    .map((_, index) => (index === active ? "minmax(0, 1.6fr)" : "minmax(0, 0.78fr)"))
    .join(" ");

  useGSAP(
    () => {
      const mm = gsap.matchMedia();
      mm.add("(prefers-reduced-motion: no-preference)", () => {
        gsap.fromTo(
          ".stage-panel-copy",
          { y: 12, opacity: 0 },
          { y: 0, opacity: 1, duration: 0.42, ease: "power2.out" },
        );
        gsap.fromTo(
          ".stage-visual",
          { opacity: 0.4, scale: 0.992 },
          { opacity: 1, scale: 1, duration: 0.45, ease: "power2.out" },
        );
      });
      return () => mm.revert();
    },
    { scope: root, dependencies: [active], revertOnUpdate: true },
  );

  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const lastIndex = pipelineStages.length - 1;
    let nextIndex = index;

    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      nextIndex = index === lastIndex ? 0 : index + 1;
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      nextIndex = index === 0 ? lastIndex : index - 1;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = lastIndex;
    } else {
      return;
    }

    event.preventDefault();
    setActive(nextIndex);
    tabRefs.current[nextIndex]?.focus();
  };

  return (
    <section
      ref={root}
      id="pipeline"
      aria-labelledby="pipeline-title"
      className="section-pad relative border-t border-line"
    >
      <div className="container-shell">
        <div className="max-w-4xl">
          <h2
            id="pipeline-title"
            className="text-[clamp(2.2rem,5.6vw,4.8rem)] font-extrabold leading-[0.95] tracking-[-0.055em]"
          >
            The signal{" "}
            <span
              className="mx-1 inline-flex h-[0.72em] w-[1.8em] translate-y-[0.04em] items-center justify-center gap-[3px] rounded-full border border-accent/40 bg-accent/10 px-3 align-middle"
              aria-hidden="true"
            >
              {[0.3, 0.72, 0.46, 0.9, 0.38].map((height, index) => (
                <span
                  key={`${height}-${index}`}
                  className="w-[2px] rounded-full bg-accent"
                  style={{ height: `${height * 100}%` }}
                />
              ))}
            </span>{" "}
            never leaves the machine.
          </h2>
          <p className="mt-6 max-w-[58ch] text-[1rem] leading-[1.65] text-muted md:text-[1.08rem]">
            Four stages between your mouth and the call. A physical microphone, one recognizer,
            one synthesizer, and a virtual source. No hosted audio anywhere in the path.
          </p>
        </div>

        <div
          className="stage-rail mt-12 md:mt-16"
          style={{ "--stage-columns": columns } as CSSProperties}
          role="tablist"
          aria-label="Signal path stages"
        >
          {pipelineStages.map((item, index) => {
            const selected = index === active;
            return (
              <button
                key={item.id}
                ref={(element) => {
                  tabRefs.current[index] = element;
                }}
                id={`stage-tab-${item.id}`}
                type="button"
                role="tab"
                aria-selected={selected}
                aria-controls="pipeline-panel"
                tabIndex={selected ? 0 : -1}
                onClick={() => setActive(index)}
                onKeyDown={(event) => onTabKeyDown(event, index)}
                className={`stage-tab group relative min-h-[112px] overflow-hidden rounded-xl border p-4 text-left transition-colors md:min-h-[148px] md:p-5 ${
                  selected
                    ? "border-accent/55 bg-accent/[0.075]"
                    : "border-line bg-panel/55 hover:border-line-strong hover:bg-panel"
                }`}
              >
                <span
                  className={`font-mono text-[10px] tracking-[0.1em] ${
                    selected ? "text-accent" : "text-faint"
                  }`}
                >
                  {item.index}
                </span>
                <span
                  className={`mt-5 block text-[1rem] font-bold tracking-[-0.03em] md:mt-8 md:text-[1.1rem] ${
                    selected ? "text-bone" : "text-muted"
                  }`}
                >
                  {item.tab}
                </span>
                <span className="mt-1 block font-mono text-[9px] uppercase tracking-[0.06em] text-faint">
                  {item.id === "route" ? "virtual source" : "signal stage"}
                </span>
                <span
                  className={`absolute bottom-0 left-0 h-[2px] bg-accent transition-[width] duration-500 ${
                    selected ? "w-full" : "w-0"
                  }`}
                  aria-hidden="true"
                />
                {selected ? (
                  <ArrowRight
                    size={16}
                    weight="bold"
                    className="absolute right-4 top-4 text-accent"
                    aria-hidden="true"
                  />
                ) : null}
              </button>
            );
          })}
        </div>

        <div
          id="pipeline-panel"
          role="tabpanel"
          aria-labelledby={`stage-tab-${stage.id}`}
          tabIndex={0}
          className="mt-2 grid grid-cols-1 gap-2 rounded-[var(--radius-panel)] border border-line bg-panel/35 p-2 lg:grid-cols-[1.05fr_0.95fr]"
        >
          <div className="stage-panel-copy flex flex-col justify-between p-5 sm:p-7">
            <div>
              <p className="mono-label">
                Stage {stage.index} / {pipelineStages.length.toString().padStart(2, "0")}
              </p>
              <h3 className="mt-4 max-w-[18ch] text-[clamp(1.6rem,3.4vw,2.5rem)] font-extrabold leading-[0.98] tracking-[-0.045em]">
                {stage.title}
              </h3>
              <p className="mt-4 max-w-[48ch] text-[0.95rem] leading-[1.65] text-muted">
                {stage.body}
              </p>
            </div>

            <div className="mt-8">
              <ul className="space-y-2.5">
                {stage.facts.map((fact) => (
                  <li key={fact} className="flex items-start gap-2.5 text-[0.86rem] text-bone/85">
                    <Check
                      size={14}
                      weight="bold"
                      className="mt-0.5 shrink-0 text-accent"
                      aria-hidden="true"
                    />
                    {fact}
                  </li>
                ))}
              </ul>
              <p className="mt-6 flex items-center gap-2 font-mono text-[10px] leading-relaxed tracking-[0.02em] text-faint">
                <ArrowLeft size={12} weight="bold" className="text-accent/70" aria-hidden="true" />
                {stage.note}
              </p>
            </div>
          </div>

          <div className="stage-visual">
            <StageVisual id={stage.id} />
          </div>
        </div>

        <p className="mt-4 text-right font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
          Diagrams describe the path. They are not app screenshots.
        </p>
      </div>
    </section>
  );
}
