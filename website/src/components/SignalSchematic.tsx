"use client";

import { useRef, useState } from "react";
import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import { ArrowRight } from "@phosphor-icons/react";

type SchematicState = "listening" | "endpoint" | "speaking";

const states: Record<
  SchematicState,
  {
    label: string;
    code: string;
    detail: string;
    activeNode: number;
    doneThrough: number;
  }
> = {
  listening: {
    label: "Listening",
    code: "MIC OPEN",
    detail: "Dex waits for 800 ms of silence, or 10 seconds without a break.",
    activeNode: 0,
    doneThrough: -1,
  },
  endpoint: {
    label: "Endpoint",
    code: "PHRASE COMMITTED",
    detail: "Parakeet has committed the live words for this phrase. Phonon is queued on the CPU.",
    activeNode: 1,
    doneThrough: 0,
  },
  speaking: {
    label: "Speaking",
    code: "MIC MUTED",
    detail: "DexVoice is playing the phrase. Overlapping speech is discarded.",
    activeNode: 3,
    doneThrough: 2,
  },
};

const nodes = ["Mic", "Parakeet", "Phonon", "Route"] as const;

function makeWavePath(amplitude: number, frequency: number, phase: number) {
  const points: string[] = [];
  for (let x = 0; x <= 520; x += 6) {
    const envelope = Math.sin((Math.PI * x) / 520);
    const primary = Math.sin(x / frequency + phase) * amplitude * envelope;
    const secondary = Math.sin(x / (frequency * 0.41) + phase * 1.7) * amplitude * 0.24 * envelope;
    const y = 80 + primary + secondary;
    points.push(`${x === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`);
  }
  return points.join(" ");
}

const wavePaths = {
  listening: makeWavePath(42, 15, 0.4),
  endpoint: makeWavePath(15, 31, 1.4),
  speaking: makeWavePath(28, 22, 2.1),
};

export function SignalSchematic() {
  const [state, setState] = useState<SchematicState>("listening");
  const root = useRef<HTMLDivElement>(null);
  const current = states[state];

  useGSAP(
    () => {
      const mm = gsap.matchMedia();
      mm.add("(prefers-reduced-motion: no-preference)", () => {
        gsap.fromTo(
          ".scope-readout",
          { y: 7, opacity: 0 },
          { y: 0, opacity: 1, duration: 0.36, ease: "power2.out" },
        );
        gsap.fromTo(
          ".scope-wave-active",
          { opacity: 0.2 },
          { opacity: 1, duration: 0.5, ease: "power1.out" },
        );
      });
      return () => mm.revert();
    },
    { scope: root, dependencies: [state], revertOnUpdate: true },
  );

  return (
    <div
      ref={root}
      className="panel-raised relative overflow-hidden"
      aria-label="Interactive phrase-state schematic"
    >
      <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-3 sm:px-5">
        <span className="font-mono text-[10px] uppercase tracking-[0.13em] text-muted">
          dex:signal / phrase state
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-line px-2.5 py-1 font-mono text-[9px] uppercase tracking-[0.12em] text-faint">
          <span className="h-1 w-1 rounded-full bg-accent" aria-hidden="true" />
          schematic
        </span>
      </div>

      <div className="schematic-grid relative px-4 pb-4 pt-5 sm:px-5 sm:pb-5">
        <div
          className="pointer-events-none absolute -right-20 top-0 h-56 w-56 rounded-full bg-accent/10 blur-3xl"
          aria-hidden="true"
        />

        <div className="relative">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="mono-label">State</p>
              <p className="mt-1.5 text-[2.35rem] font-extrabold leading-[0.9] tracking-[-0.055em] text-bone sm:text-[2.6rem]">
                {current.label}
              </p>
            </div>
            <div className="flex items-center gap-1.5 pt-1" aria-hidden="true">
              {[28, 19, 34, 24, 40, 18, 30, 22].map((height, index) => (
                <span
                  key={`${height}-${index}`}
                  className="w-[3px] rounded-full bg-bone/30 transition-opacity duration-300"
                  style={{
                    height: `${height}px`,
                    opacity: state === "listening" ? 0.42 + index * 0.05 : 0.2,
                  }}
                />
              ))}
            </div>
          </div>

          <div className="relative mt-4 h-[148px] overflow-hidden rounded-xl border border-line bg-ink-deep/60">
            <div className="absolute inset-x-4 top-1/2 h-px bg-bone/10" aria-hidden="true" />
            <div className="absolute inset-y-3 left-1/2 w-px bg-bone/10" aria-hidden="true" />
            <svg
              viewBox="0 0 520 160"
              className="absolute inset-0 h-full w-full"
              preserveAspectRatio="none"
              aria-hidden="true"
            >
              {(["listening", "endpoint", "speaking"] as SchematicState[]).map((wave) => (
                <path
                  key={wave}
                  d={wavePaths[wave]}
                  fill="none"
                  stroke={wave === "speaking" ? "#f2eadf" : "#ea7a4c"}
                  strokeWidth={wave === state ? 2 : 1}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  vectorEffect="non-scaling-stroke"
                  className={
                    wave === state
                      ? `scope-wave-active ${wave === "listening" ? "wave-drift" : ""}`
                      : ""
                  }
                  opacity={wave === state ? 1 : 0}
                />
              ))}
            </svg>
            <div className="absolute inset-x-0 bottom-0 flex items-center justify-between px-3 py-2 font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
              <span>input envelope</span>
              <span>{current.code}</span>
            </div>
          </div>

          <div className="mt-4 grid grid-cols-4 gap-1.5" aria-hidden="true">
            {nodes.map((node, index) => {
              const active = index === current.activeNode;
              const done = index <= current.doneThrough;
              return (
                <div
                  key={node}
                  className={`rounded-lg border px-2 py-2 transition-colors duration-300 ${
                    active
                      ? "border-accent/55 bg-accent/10"
                      : done
                        ? "border-line bg-bone/[0.035]"
                        : "border-line/70 bg-transparent"
                  }`}
                >
                  <span
                    className={`mb-2 block h-1 w-1 rounded-full ${
                      active ? "bg-accent" : done ? "bg-bone/60" : "bg-bone/20"
                    }`}
                  />
                  <span
                    className={`block font-mono text-[8px] uppercase tracking-[0.08em] ${
                      active ? "text-bone" : done ? "text-muted" : "text-faint"
                    }`}
                  >
                    {node}
                  </span>
                </div>
              );
            })}
          </div>

          <div className="scope-readout mt-4 flex min-h-[62px] items-start gap-2.5 rounded-xl border border-line bg-ink/40 px-3.5 py-3">
            <ArrowRight
              size={15}
              weight="bold"
              className="mt-0.5 shrink-0 text-accent"
              aria-hidden="true"
            />
            <p className="text-[0.82rem] leading-[1.45] text-muted" aria-live="polite">
              {current.detail}
            </p>
          </div>

          <div
            className="mt-4 grid grid-cols-3 gap-1.5"
            role="group"
            aria-label="Choose a phrase state"
          >
            {(Object.keys(states) as SchematicState[]).map((key) => {
              const active = state === key;
              return (
                <button
                  key={key}
                  type="button"
                  aria-pressed={active}
                  onClick={() => setState(key)}
                  className={`min-h-10 rounded-lg border px-2 py-2 font-mono text-[9px] uppercase tracking-[0.08em] transition-colors ${
                    active
                      ? "border-accent bg-accent text-ink"
                      : "border-line text-muted hover:border-line-strong hover:text-bone"
                  }`}
                >
                  {states[key].label}
                </button>
              );
            })}
          </div>

          <p className="mt-3 font-mono text-[9px] leading-relaxed tracking-[0.02em] text-faint">
            Interactive diagram. No audio is captured or played on this page.
          </p>
        </div>
      </div>
    </div>
  );
}
