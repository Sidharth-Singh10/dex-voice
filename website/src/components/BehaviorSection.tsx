"use client";

import { useRef } from "react";
import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import { ArrowDown } from "@phosphor-icons/react";
import { behaviorText } from "@/lib/content";

const words = behaviorText.split(" ");

export function BehaviorSection() {
  const pin = useRef<HTMLDivElement>(null);

  useGSAP(
    () => {
      gsap.registerPlugin(ScrollTrigger);

      const mm = gsap.matchMedia();
      mm.add("(min-width: 1024px) and (prefers-reduced-motion: no-preference)", () => {
        const wordElements = gsap.utils.toArray<HTMLElement>(".behavior-word");
        gsap.set(wordElements, { opacity: 0.16 });

        const timeline = gsap.timeline({
          scrollTrigger: {
            trigger: pin.current,
            start: "top top",
            end: "+=85%",
            pin: true,
            scrub: 0.65,
            anticipatePin: 1,
            invalidateOnRefresh: true,
          },
        });

        timeline.to(wordElements, {
          opacity: 1,
          ease: "none",
          stagger: 0.055,
          duration: 1,
        });
      });

      return () => mm.revert();
    },
    { scope: pin },
  );

  return (
    <section
      id="limits"
      aria-labelledby="limits-title"
      className="pin-section relative border-t border-line bg-ink-deep/40"
    >
      <div
        className="pointer-events-none absolute left-1/2 top-1/2 h-[42rem] w-[42rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent/[0.035] blur-3xl"
        aria-hidden="true"
      />
      <div
        ref={pin}
        data-pin="behavior"
        className="relative flex min-h-[100dvh] items-center py-24 md:py-28"
      >
        <div className="container-shell w-full">
          <div className="grid grid-cols-1 items-center gap-14 lg:grid-cols-[0.78fr_1.22fr] lg:gap-20">
            <div className="max-w-[31rem]">
              <h2
                id="limits-title"
                className="text-[clamp(2.5rem,6vw,5.25rem)] font-extrabold leading-[0.92] tracking-[-0.06em]"
              >
                It waits for{" "}
                <span className="relative inline-block">
                  the pause.
                  <span
                    className="absolute -bottom-1 left-0 h-[3px] w-full origin-left bg-accent lg:h-[4px]"
                    aria-hidden="true"
                  />
                </span>
              </h2>
              <p className="mt-7 max-w-[34ch] text-[1rem] leading-[1.65] text-muted">
                The phrase is the unit. Not the word, and not the millisecond. That choice
                defines the behavior, and the latency.
              </p>
              <div className="behavior-scroll-cue mt-8 hidden items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.11em] text-faint lg:flex">
                <ArrowDown
                  size={14}
                  weight="bold"
                  className="scroll-cue text-accent"
                  aria-hidden="true"
                />
                Scroll to resolve the phrase
              </div>
            </div>

            <div className="relative">
              <p
                className="max-w-[52ch] text-[clamp(1.42rem,2.9vw,2.25rem)] font-medium leading-[1.28] tracking-[-0.035em] text-bone"
                aria-label={behaviorText}
              >
                <span className="sr-only">{behaviorText}</span>
                {words.map((word, index) => {
                  const isNumber = word === "800" || word === "10";
                  return (
                    <span
                      key={`${word}-${index}`}
                      className={`behavior-word ${isNumber ? "text-accent" : ""}`}
                      aria-hidden="true"
                    >
                      {word}
                      {index < words.length - 1 ? " " : ""}
                    </span>
                  );
                })}
              </p>

              <div className="mt-10 border-t border-line pt-5">
                <div className="flex flex-wrap items-center gap-x-6 gap-y-2 font-mono text-[9px] uppercase tracking-[0.1em] text-faint">
                  <span className="flex items-center gap-2">
                    <span className="h-1.5 w-1.5 rounded-full bg-bone/50" aria-hidden="true" />
                    Mic open
                  </span>
                  <span className="flex items-center gap-2">
                    <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
                    Phrase boundary
                  </span>
                  <span className="flex items-center gap-2">
                    <span className="h-1.5 w-1.5 rounded-full border border-bone/30" aria-hidden="true" />
                    Mic muted
                  </span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
