import { describe, expect, it } from "vitest";
import {
  behaviorText,
  blockedItems,
  envelopeFacts,
  pipelineStages,
  prerequisites,
  resynthesisNote,
  setupSteps,
  shutdownCleanup,
  troubleshootingCommands,
} from "@/lib/content";
import { repoFile, repoUrl } from "@/lib/site";

describe("showcase content", () => {
  it("keeps the documented four-stage signal path", () => {
    expect(pipelineStages.map((stage) => stage.id)).toEqual(["mic", "asr", "tts", "route"]);
    expect(pipelineStages.map((stage) => stage.tab)).toEqual([
      "Mic",
      "Parakeet",
      "Phonon",
      "PipeWire",
    ]);
  });

  it("states the phrase boundary and the unmet latency target", () => {
    expect(behaviorText).toContain("800 ms");
    expect(behaviorText).toContain("10 seconds");
    expect(blockedItems.map((item) => item.title)).toContain("Under 500 ms");
    expect(blockedItems.map((item) => item.title)).toContain("A Windows Rust build");
    expect(blockedItems.map((item) => item.title)).toContain("Public distribution");
    expect(blockedItems.map((item) => item.title)).toContain("Shutdown cleanup");
    expect(shutdownCleanup.detail).toContain("deliberately closing the app");
    expect(shutdownCleanup.detail).toContain("ordinary-operation crash");
    expect(shutdownCleanup.detail).toContain("teardown cleanup");
    expect(blockedItems.find((item) => item.title === "Shutdown cleanup")?.severity).toBe(
      "verify",
    );
  });

  it("describes live word commits and scoped capture compatibility", () => {
    const asr = pipelineStages.find((stage) => stage.id === "asr");
    const route = pipelineStages.find((stage) => stage.id === "route");

    expect(asr?.title).toBe("Parakeet commits words live");
    expect(asr?.body).toContain("Words are committed as they are recognized");
    expect(asr?.body).toContain("handed to synthesis");
    expect(asr?.note).toContain("Recognition streams");
    expect(asr?.note).not.toContain("phrase-based");

    expect(route?.body).toContain("expected to list it");
    expect(route?.body).toContain("Verify the app's microphone permissions");
    expect(route?.body).not.toMatch(/any other app/i);
    expect(route?.note).toContain("reopen or reload");
    expect(route?.note).not.toMatch(/most enumerate/i);
  });

  it("keeps the setup state and tested toolchain precise", () => {
    const runStep = setupSteps.find((step) => step.id === "run");
    const callStep = setupSteps.find((step) => step.id === "call");

    expect(runStep?.detail).toContain("wait for Listening");
    expect(runStep?.detail).not.toContain("Running");
    expect(callStep?.detail).toContain("if it lists normal PipeWire or PulseAudio");
    expect(callStep?.detail).toContain("reopen or reload");
    expect(prerequisites).toContain("Rust 1.98.1 tested with edition 2024");
    expect(prerequisites.join(" ")).not.toContain("1.85");
  });

  it("qualifies the endpoint and includes the re-synthesis limit", () => {
    expect(envelopeFacts[0].detail).toContain("reduce clipped words");
    expect(envelopeFacts[0].detail).toContain("does not guarantee");
    expect(envelopeFacts[0].detail).not.toContain("not split off");
    expect(resynthesisNote.lead).toBe("Re-synthesis, not a filter.");
    expect(resynthesisNote.detail).toContain("emotion");
    expect(resynthesisNote.detail).toContain("delivery");
  });

  it("rejects the removed driver-crash and universal app claims", () => {
    const content = JSON.stringify({
      behaviorText,
      blockedItems,
      envelopeFacts,
      pipelineStages,
      prerequisites,
      resynthesisNote,
      setupSteps,
      shutdownCleanup,
      troubleshootingCommands,
    });

    expect(content).not.toMatch(/driver issue/i);
    expect(content).not.toMatch(/Vulkan stability/i);
    expect(content).not.toMatch(/any other app/i);
    expect(content).not.toMatch(/most enumerate/i);
    expect(content).not.toMatch(/Rust 1\.85/i);
    expect(content).not.toMatch(/Wait for Running/i);
    expect(content).not.toMatch(/not split off/i);
    expect(content).not.toMatch(/complete words are committed/i);
    expect(content).not.toMatch(/Discord|Meet/);
  });

  it("uses the commands documented in the Linux runbook", () => {
    const commands = [
      ...setupSteps.map((step) => step.command),
      ...troubleshootingCommands.map((item) => item.command),
    ].filter((command): command is string => Boolean(command));

    expect(commands).toContain("cd rust");
    expect(commands).toContain("./scripts/install-models.sh");
    expect(commands).toContain("cargo build --release -p dex-voice");
    expect(commands).toContain("./scripts/setup-virtual-mic.sh");
    expect(commands).toContain("./target/release/dex-voice");
  });

  it("links only the project repository and no download URL", () => {
    const content = JSON.stringify({
      behaviorText,
      blockedItems,
      pipelineStages,
      setupSteps,
      troubleshootingCommands,
    });
    const urls = [...content.matchAll(/https?:\/\/[^"\\\s]+/g)].map((match) => match[0]);

    expect(urls.length).toBeGreaterThan(0);
    for (const url of urls) {
      expect(url.startsWith(repoUrl)).toBe(true);
    }
    expect(content).not.toMatch(/\/(releases|download|installer)/i);
    expect(content).not.toMatch(/github\.com\/(?!Sidharth-Singh10\/dex-voice)/i);
  });

  it("builds repository file links on the main branch", () => {
    expect(repoFile("rust/LINUX.md")).toBe(
      "https://github.com/Sidharth-Singh10/dex-voice/blob/main/rust/LINUX.md",
    );
    expect(repoFile("README.md")).toBe(`${repoUrl}/blob/main/README.md`);
  });
});
