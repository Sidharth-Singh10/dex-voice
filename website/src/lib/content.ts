export const navLinks = [
  { href: "#pipeline", label: "Signal path" },
  { href: "#voices", label: "Voices" },
  { href: "#limits", label: "Limits" },
  { href: "#setup", label: "Linux" },
] as const;

export const truthFacts = [
  {
    label: "Linux-first",
    detail: "End-to-end on one development laptop",
  },
  {
    label: "Local only",
    detail: "No cloud, account, or telemetry",
  },
  {
    label: "Half-duplex",
    detail: "The mic pauses while Dex speaks",
  },
  {
    label: "No public build",
    detail: "Run from a local checkout",
  },
] as const;

export type StageId = "mic" | "asr" | "tts" | "route";

export type PipelineStage = {
  id: StageId;
  index: string;
  tab: string;
  title: string;
  summary: string;
  body: string;
  facts: string[];
  note: string;
};

export const pipelineStages: PipelineStage[] = [
  {
    id: "mic",
    index: "01",
    tab: "Mic",
    title: "A physical microphone",
    summary: "cpal captures the input you choose.",
    body: "Dex opens the selected physical microphone through cpal on PipeWire's PulseAudio host. It never changes your system's default speaker or microphone. Capture and playback were verified with a live fixture on the development laptop.",
    facts: [
      "Selected explicitly in Settings",
      "20 ms playback buffer requested",
      "System defaults untouched",
    ],
    note: "The signal starts here, not in a browser tab.",
  },
  {
    id: "asr",
    index: "02",
    tab: "Parakeet",
    title: "Parakeet commits words live",
    summary: "F16 GGUF, transcribe.cpp, Vulkan.",
    body: "NVIDIA Parakeet Unified runs as an F16 GGUF through transcribe.cpp on Vulkan. Words are committed as they are recognized. The phrase boundary is when the accumulated transcript is handed to synthesis, not when recognition starts. The NVIDIA device is selected explicitly rather than falling back silently to another GPU.",
    facts: [
      "Live word commits during speech",
      "Phrase boundary hands text to TTS",
      "NVIDIA device selected explicitly",
    ],
    note: "Recognition streams; synthesis starts at the phrase boundary.",
  },
  {
    id: "tts",
    index: "03",
    tab: "Phonon",
    title: "Phonon speaks it back",
    summary: "CPU synthesis, three installed voices.",
    body: "Phonon 31M re-synthesizes the phrase text on the CPU. The voice is independent of the recognition model. Default, Zoey, and Juniper are installed by the Linux model script from a pinned catalog.",
    facts: [
      "Phonon 31M on CPU",
      "Default, Zoey, and Juniper",
      "Voice choice does not change ASR",
    ],
    note: "Complete speech currently outranks low latency.",
  },
  {
    id: "route",
    index: "04",
    tab: "PipeWire",
    title: "DexMicrophone as a normal source",
    summary: "A DexVoice sink, remapped as a microphone.",
    body: "The setup script creates a null sink named dex_voice and remaps it to dex_mic, displayed as DexMicrophone. Apps that accept a normal PipeWire or PulseAudio capture source are expected to list it. Verify the app's microphone permissions and device list before relying on it.",
    facts: [
      "Null sink: dex_voice",
      "Remapped source: DexMicrophone",
      "App listing and permissions need verification",
    ],
    note: "If the app was already open, reopen or reload its microphone list and reselect the source.",
  },
];

export const voices = [
  {
    name: "Default",
    meta: "Included with the Linux model script",
    description:
      "The selected Phonon checkpoint's default English voice. It is the baseline installed voice, not a separate model.",
    stamp: "voice / default",
  },
  {
    name: "Zoey",
    meta: "Additional English voice embedding",
    description:
      "An English voice embedding installed from the pinned catalog. Voice choice is separate from Parakeet and does not alter recognition.",
    stamp: "voice / zoey",
  },
  {
    name: "Juniper",
    meta: "Additional English voice embedding",
    description:
      "A second English embedding from the same pinned catalog. It runs through the same CPU synthesis path as the other voices.",
    stamp: "voice / juniper",
  },
] as const;

export const behaviorText =
  "Dex keeps the microphone open until it hears 800 ms of silence, or 10 seconds without a break. Then it stops listening, synthesizes the phrase, and plays it. Anything you say while Dex is speaking is not captured.";

export const envelopeFacts = [
  {
    value: "800 ms",
    label: "quiet before Dex speaks",
    detail:
      "A phrase boundary, not a word boundary. Short clause pauses are held together to reduce clipped words; this does not guarantee that every final word is preserved.",
  },
  {
    value: "10 s",
    label: "maximum uninterrupted phrase",
    detail:
      "If you keep talking without a pause, Dex closes the phrase at the cap instead of waiting forever.",
  },
  {
    value: "30 s",
    label: "bounded output buffer",
    detail:
      "Complete speech wins over low latency in this build. Long speech can accumulate audible delay in the queue.",
  },
] as const;

export const workingItems = [
  {
    title: "The Linux route",
    detail:
      "A device-free self-test passes, and a fixture run carried synthesized speech through to DexMicrophone on the development laptop.",
  },
  {
    title: "Audio devices",
    detail:
      "Live capture and playback were verified with a physical microphone. Virtual sinks and remapped sources enumerate as normal PipeWire devices.",
  },
  {
    title: "Three voices",
    detail:
      "Default, Zoey, and Juniper install from the pinned catalog and pass the device-free fixture test.",
  },
  {
    title: "The Rune shell",
    detail:
      "Device selection and settings are built in native Rune. The window exposes Start, Stop, a phrase state, and a Clear action.",
  },
] as const;

export const shutdownCleanup = {
  title: "Shutdown cleanup",
  detail:
    "A crash has been reported when deliberately closing the app. It does not establish an ordinary-operation crash or a proven driver defect; teardown cleanup still needs verification.",
} as const;

export const resynthesisNote = {
  lead: "Re-synthesis, not a filter.",
  detail:
    "The generated voice carries the recognized text. The original emotion, timing, and delivery are not preserved.",
} as const;

export type StatusItem = {
  title: string;
  detail: string;
  severity?: "open" | "verify";
};

export const blockedItems: StatusItem[] = [
  {
    title: "Under 500 ms",
    detail:
      "The strict end-to-end target is not met. Latency tuning is deferred; the phrase wait and synthesis backlog dominate.",
  },
  {
    title: "A Windows Rust build",
    detail:
      "There is no Windows Rust executable or tested Windows audio route. This is a Linux-only experimental build.",
  },
  {
    title: "Public distribution",
    detail:
      "There is no public download, Linux package, installer, or hosted service. No account or backend exists.",
  },
  {
    ...shutdownCleanup,
    severity: "verify",
  },
];

export type SetupStep = {
  id: string;
  title: string;
  detail: string;
  command: string | null;
};

export const setupSteps: SetupStep[] = [
  {
    id: "enter",
    title: "Enter the Rust workspace",
    detail:
      "Start from a local checkout of the repository. This page does not link to a public clone URL or release package.",
    command: "cd rust",
  },
  {
    id: "models",
    title: "Install the pinned models",
    detail:
      "Downloads about 1.3 GB by default, verifies SHA-256, and installs the F16 Parakeet model plus Default, Zoey, and Juniper. Use DEX_MODELS to choose another directory.",
    command: "./scripts/install-models.sh",
  },
  {
    id: "build",
    title: "Build the desktop app",
    detail:
      "The first build takes several minutes. Vulkan headers, libcblas, libpulse, CMake, Ninja, glslc, and a C++17 compiler must be available first.",
    command: "cargo build --release -p dex-voice",
  },
  {
    id: "route",
    title: "Create the virtual microphone",
    detail:
      "Idempotent. Creates the dex_voice sink and the remapped DexMicrophone source. It does not change your normal speaker or microphone defaults.",
    command: "./scripts/setup-virtual-mic.sh",
  },
  {
    id: "run",
    title: "Run Dex Voice",
    detail:
      "Open Settings, choose your real mic and DexVoice as the virtual microphone output, then pick a voice. Save and press Start, then wait for Listening before speaking.",
    command: "./target/release/dex-voice",
  },
  {
    id: "call",
    title: "Select DexMicrophone in the call",
    detail:
      "Choose DexMicrophone in the calling app if it lists normal PipeWire or PulseAudio capture sources. Verify the app's microphone permissions and device list. If it was already open, reopen or reload its microphone list and reselect the source.",
    command: null,
  },
];

export const troubleshootingCommands = [
  {
    label: "List audio device IDs and names",
    command: "cargo run --release -p dex-audio --bin devices",
  },
  {
    label: "Find the Dex routing modules",
    command: "pactl list short modules | grep dex",
  },
  {
    label: "Remove a module when you are done",
    command: "pactl unload-module <module-ID>",
    note: "Replace <module-ID> with the ID printed by the command above.",
  },
] as const;

export const prerequisites = [
  "Rust 1.98.1 tested with edition 2024",
  "CMake, Ninja, a C++17 compiler, and glslc",
  "Vulkan loader, drivers, and build headers",
  "libcblas and libpulse development libraries",
  "An NVIDIA GPU selected explicitly through Vulkan",
] as const;
