# Dex Voice showcase

A single-page, statically rendered showcase for Dex Voice, a Linux-first experimental local voice changer. It explains the real signal path, the three installed voices, the phrase behavior, the unresolved limits, and the Linux setup route.

This is a showcase, not the application. There is no public download, installer, hosted service, account, backend, analytics, or telemetry. The page does not invent a repository URL or a release.

## Stack

- Next.js 16.4.5 with the App Router and Turbopack
- React 19.3.1 and TypeScript 5.9
- Tailwind CSS 4.3
- GSAP 3.15 with `@gsap/react` and ScrollTrigger
- Phosphor Icons
- Playwright for end-to-end checks, Vitest and Testing Library for component checks

## Run locally

Requires Node.js 20.9 or newer.

```sh
cd website
npm install
npm run dev
```

Open <http://localhost:3000>.

## Build and preview

```sh
cd website
npm run build
npm run start
```

`npm run start` serves the production build on port 3000 by default. The page is prerendered as static content and needs no environment variables.

## Vercel

1. Import the repository into Vercel.
2. Set the project root to `website` if the repository root contains the rest of the Dex source.
3. Keep the framework preset on Next.js.
4. Use `npm run build` as the build command and leave the output directory on the Next.js default (`.next`).
5. Deploy. No environment variables, functions, database, or secrets are required.

The site is also deployable to any Node host that supports `npm run build` and `npm run start`.

## Checks

```sh
cd website
npm run lint
npm run typecheck
npm test
npm run build
npm run test:e2e
```

`npm run test:e2e` starts a local dev server on port 3100 unless one is already running. It exercises the mobile menu, the state schematic, the signal-path tabs, the clipboard copy path, the scoped shutdown and re-synthesis caveats, reduced motion, and horizontal overflow at several viewport widths.

## Structure

```text
src/app/            App Router layout, page, global styles, favicon, local fonts
src/components/     Page sections and interactive components
src/lib/content.ts  All product copy, stage data, setup commands, and status rows
tests/              Vitest component and content checks
e2e/                Playwright browser checks
```

## Content and interaction notes

- The hero schematic is interactive and explicitly labeled as a diagram. It never pretends to capture or play audio.
- Stages are keyboard-operable tabs. The mobile navigation is a disclosure that closes on Escape and is removed from the tab order when closed.
- Copy buttons copy the exact command from the Linux runbook and fall back to a temporary textarea if the Clipboard API is unavailable.
- Motion is disabled or degraded when `prefers-reduced-motion: reduce` is set. GSAP work is scoped and cleaned up through `useGSAP`.
- Geometry and diagrams are custom inline SVG/CSS. The page uses no stock photography or fabricated product screenshots.
- The palette is intentionally dark and theme-locked: warm near-black, bone text, and one ember accent.
- Setup names Rust 1.98.1 (edition 2024) as the tested toolchain because the Rust workspace does not declare an MSRV.
- The shutdown note is scoped to a reported crash on intentional close. It is not a general stability claim, and calling-app compatibility is presented as something to verify per app.
- The limits chapter states that the app re-synthesizes text and does not preserve the original emotion, timing, or delivery. The site contains no audio previews.

## Fonts

Cabinet Grotesk is self-hosted from Fontshare under the ITF Free Font License. See `src/app/fonts/README.md`. Geist Mono is loaded through `next/font/google` at build time and self-hosted by Next.js.

## Product truth

The site repeats the current limitations rather than hiding them: the <500 ms latency target is not met, synthesis is phrase-based and half-duplex, overlapping speech is discarded, a crash has been reported on deliberate close with teardown cleanup still unverified, there is no Windows Rust build, and no public package exists. The measured spike numbers are intentionally left out of this showcase.

The Linux commands in the setup section mirror `rust/LINUX.md` and `rust/README.md`. If those runbook commands change, update `src/lib/content.ts` and the content test together.
