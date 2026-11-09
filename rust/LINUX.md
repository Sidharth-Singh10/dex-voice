# Linux runbook

Status (2026-11-08): the Rust app works end-to-end on this laptop with F16
Parakeet/Vulkan, Phonon CPU, and PipeWire's PulseAudio-compatible virtual sinks.
Latency is **not** ready for a live conversation; complete speech currently
takes priority over minimizing the output queue.

1. Install the [build dependencies](README.md#prerequisites). From `rust/`, run
   `./scripts/install-models.sh` and `cargo build --release -p dex-voice`.
2. Create the virtual microphone with `./scripts/setup-virtual-mic.sh`. It is
   idempotent and creates two things: null sink `dex_voice` (where the app
   writes) and a **remapped source** `dex_mic`, described as `DexMicrophone`.
   The remap matters: Discord and some browsers hide plain `.monitor` sources,
   while a remapped source is listed like any microphone. Keep your normal
   speaker/mic as system defaults; the script does not change them.
3. Run `./target/release/dex-voice`, open **Settings**, and pick your real mic
   under Microphone and **DexVoice** under Virtual microphone output. Pick
   Default, Zoey, or Juniper (installed by the Linux model script). Save and
   Start; wait for **Listening** before speaking.
4. Select **DexMicrophone** as the microphone in the receiving application.
   Restart an app that was already running when the device was created — most
   enumerate devices only at startup. To stop, use the app's **Stop** button.
   Remove the devices later with `pactl unload-module <module-ID>` for each
   module (`pactl list short modules | grep dex`).
5. For a repeatable device-free acceptance run:
   `./target/release/dex-voice --self-test self-test.json`. Check `ok: true`,
   a transcript ending in “portrait”, `peak > 0`, and zero input/output drops.
   This uses an embedded fixture, not the physical mic or virtual audio device.

## Verified on this machine

- Capture and playback now accept every `cpal` sample format (including the
  default mic's 32-bit integer stream); previously the app failed with
  `unsupported input sample format I32`. A live run opened
  `JadeAudio JA11 Mono` at 48 kHz mono and transcribed immediately.

- `cpal` PulseAudio host lists real PipeWire sinks/sources, including virtual
  sink `dex_out` and capture source `dex_in.monitor`; explicit device IDs work
  without changing either system default.
- A headless Rune Settings interaction saved the selected device ID and model
  paths to an isolated settings file; the actual window was reviewed through
  offscreen snapshots rather than a desktop screenshot.
- Live fixture through `dex_in` was recognized through “portrait”; recording
  `dex_out.monitor` contained synthesized speech through the last word. A
  separate ASR pass on the output heard “old old portrait” near the end, so
  speech is audible but synthesis/group boundaries still need quality work.
- `--self-test` recognized 23 words and produced 460,318 PCM samples, with
  zero input/output drops. Full workspace release tests and strict Clippy pass.
- Zoey and Juniper embeddings from a pinned Apache-2.0 voice catalog also
  passed the device-free fixture test with zero input/output drops.
- Tests use disposable virtual sinks; they were unloaded afterward. No audio
  defaults were changed during this explicit-device test.
- Virtual microphone is a `module-remap-source` (`dex_mic`, "DexMicrophone"),
  so it appears in the PulseAudio API, GStreamer's `gst-device-monitor`, and the
  PipeWire node graph as a normal capture device. Playing a WAV into `dex_voice`
  produced matching PCM on `dex_mic` (peak 0.416), confirming the route carries
  audio rather than merely appearing in lists.
- The window shows `Listening` / `Speaking · mic muted` and has a Clear button;
  `--self-test` and the fixture runs confirm the final spoken word survives the
  phrase boundary (recorded sink transcribed through "portrait").
- Half-duplex echo guard verified with a deliberate digital loop
  (`dex_voice.monitor` → loopback sink → app microphone): one phrase was
  recognized, spoken once, and the transcript then stayed fixed instead of
  repeating. Before the guard it grew without bound.
- Playback/capture now request a 20 ms PulseAudio buffer instead of the server
  default (about two seconds), so audio no longer keeps playing after the queue
  is drained. This also removes a large hidden latency.

## Known limitations

- **Half-duplex.** The app stops listening while its own speech is queued,
  then waits for the 500 ms post-pop guard before listening again. It will not
  talk over you, but speech that overlaps its own voice is discarded. Use
  headphones if another app plays DexMicrophone on your speakers; there is no
  echo cancellation.
- Speech is spoken at a pause boundary (~800 ms of silence, plus a 10 s cap for
  uninterrupted talk), not word-by-word. Clause pauses below that are kept
  together, so a sentence's final word is not split off or dropped. Latency is
  therefore well above the <500 ms target.
- p50/p95 measure the time from the first non-empty recognition hypothesis for a phrase (committed or tentative) to the first sample pushed into the output queue. They include the phrase-boundary wait and synthesis time and exclude playback/device and acoustic latency. A phrase that produces no PCM records no sample. p95 is the 95th percentile, not a “slowest 5%” threshold. Queue and dropped
  seconds are read from the live PCM queue between engine snapshots.
- The 30-second output buffer can accumulate delay on long speech. No
  cancellation or simultaneous talk handling yet.
- Settings uses explicit pickers for microphone, virtual output, and voice; there
  is no cycle-through-devices control. **Refresh** re-enumerates devices without
  opening a stream or changing system defaults. Start refuses a blank/default
  output but cannot verify a selected physical speaker is a virtual device.
- The console enumerates devices once at launch only to name the routing panel;
  it does not open capture or playback until Start. If enumeration fails, the
  routing panel says so and Settings offers Refresh.
- **Bounded shutdown.** Stop runs a control thread with bounded joins (about 8 s
  total); the UI never joins workers. If a worker fails to exit, the engine is
  released with a `shutdown_incomplete` warning and `orphan_loader_exits` is
  reported. A failed engine stays attached until Stop releases it.
- **Failed-attached.** While failed, the engine stays attached and Start is
  refused until Stop releases it; the failure domain (synthesis vs. device) and
  the terminal state are preserved.

- **Device recovery.** Playback errors, a missing output device, or a queue that
  stops draining for ~500 ms are treated as device loss: the app drops the old
  playback, clears and counts the queue, and reopens the device once (by ID,
  then by label/name). A second loss or a failed reopen marks the device failed
  and stops the engine; synthesis/ASR are not restarted. A recreated device with
  a different ID is resolved by label, otherwise the run ends in a documented
  Failed state.
