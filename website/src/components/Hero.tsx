"use client";

import { useRef } from "react";
import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import { ArrowDown, ArrowUpRight, GithubLogo } from "@phosphor-icons/react";
import { repoUrl } from "@/lib/site";
import { SignalSchematic } from "@/components/SignalSchematic";

export function Hero() {
  const root = useRef<HTMLElement>(null);

  useGSAP(
    () => {
      const mm = gsap.matchMedia();
      mm.add("(prefers-reduced-motion: no-preference)", () => {
        const timeline = gsap.timeline({
          defaults: { ease: "power3.out" },
          delay: 0.12,
        });

        timeline
          .from(".hero-title-line", {
            y: 34,
            opacity: 0,
            duration: 0.9,
            stagger: 0.1,
          })
          .from(
            ".hero-copy-item",
            { y: 16, opacity: 0, duration: 0.62, stagger: 0.08 },
            "-=0.48",
          )
          .from(
            ".hero-panel",
            { y: 28, opacity: 0, scale: 0.985, duration: 0.9 },
            "-=0.55",
          );
      });
      return () => mm.revert();
    },
    { scope: root },
  );

  return (
    <section
      ref={root}
      id="top"
      aria-labelledby="hero-title"
      className="relative flex min-h-[100dvh] items-center overflow-hidden pb-20 pt-24 md:pb-24"
    >
      <div className="hero-lines pointer-events-none absolute inset-0" aria-hidden="true" />
      <div
        className="pointer-events-none absolute -right-40 top-12 h-[34rem] w-[34rem] rounded-full border border-bone/5"
        aria-hidden="true"
      />
      <div
        className="pointer-events-none absolute -right-24 top-28 h-[24rem] w-[24rem] rounded-full border border-accent/10"
        aria-hidden="true"
      />
      <div
        className="pointer-events-none absolute left-[46%] top-1/3 h-52 w-52 rounded-full bg-accent/[0.06] blur-3xl"
        aria-hidden="true"
      />

      <div className="container-shell relative">
        <div className="grid grid-cols-1 items-center gap-14 lg:grid-cols-[1.02fr_0.98fr] lg:gap-8 xl:gap-12">
          <div className="relative z-10 max-w-[42rem] lg:pb-12">
            <h1
              id="hero-title"
              className="max-w-4xl text-[clamp(2rem,9.8vw,5.5rem)] font-extrabold leading-[0.9] tracking-[-0.062em]"
            >
              <span className="hero-title-line block">Your voice,</span>
              <span className="hero-title-line block">
                <span className="text-accent">rebuilt</span> locally.
              </span>
            </h1>

            <p className="hero-copy-item mt-7 max-w-[40ch] text-[1.02rem] leading-[1.6] text-muted md:text-[1.12rem]">
              An experimental Linux voice changer. It waits for your pause, then speaks the
              phrase in another voice.
            </p>

            <div className="hero-copy-item mt-8 flex flex-col gap-3 sm:flex-row sm:items-center">
              <a href="#setup" className="btn btn-primary w-full sm:w-auto">
                Set up on Linux
                <ArrowUpRight size={17} weight="bold" aria-hidden="true" />
              </a>
              <a href="#pipeline" className="btn btn-secondary w-full sm:w-auto">
                See the signal path
                <ArrowDown size={16} weight="bold" aria-hidden="true" />
              </a>
            </div>

            <a
              href={repoUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="hero-copy-item mt-6 inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.12em] text-faint transition-colors hover:text-bone"
            >
              <GithubLogo size={14} weight="bold" aria-hidden="true" />
              View source on GitHub
            </a>
          </div>

          <div className="hero-panel relative z-10 lg:-mr-3 lg:translate-y-7 xl:-mr-8">
            <div
              className="pointer-events-none absolute -inset-5 -z-10 rounded-[28px] bg-accent/[0.045] blur-2xl"
              aria-hidden="true"
            />
            <SignalSchematic />
          </div>
        </div>
      </div>
    </section>
  );
}
