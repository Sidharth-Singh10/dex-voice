//! Presentation: the console, routing/signal panels, and the settings page.
//!
//! The pipeline lives in `main`; this module only reads [`State`] and dispatches
//! the same handlers the app has always had. Counters are the real engine
//! values, and each one says what it measures.

use std::path::PathBuf;

use dex_core::{FailureDomain, State as EngineState};
use rune::{Color, Cx, El, Icon, ScrollSpec, TextStyle, Tx, col, el, icon, oklch, row, text};
use rune_kit::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, Loader, PaletteFocus,
    PaletteSeed, Select, SelectAppearance, TextInput,
};

use crate::{SelectMenu, State, config, device_label, device_labels, voice_labels};

// ── Icons ────────────────────────────────────────────────────────────────────

static PLAY: Icon = Icon {
    name: "dex-play",
    view: 16.0,
    body: r#"<path d="M4.75 2.75L12.5 8L4.75 13.25V2.75Z" fill="currentColor"/>"#,
};

static STOP: Icon = Icon {
    name: "dex-stop",
    view: 16.0,
    body: r#"<rect x="4" y="4" width="8" height="8" rx="1.25" fill="currentColor"/>"#,
};

static SPEAKER: Icon = Icon {
    name: "dex-speaker",
    view: 16.0,
    body: r#"<path d="M2.5 6H5L8.5 3.25V12.75L5 10H2.5V6Z" stroke="currentColor" stroke-linejoin="round"/><path d="M10.75 5.5C11.6 6.25 11.6 9.75 10.75 10.5M12.5 3.75C14.2 5.25 14.2 10.75 12.5 12.25" stroke="currentColor" stroke-linecap="round"/>"#,
};

static HEADPHONES: Icon = Icon {
    name: "dex-headphones",
    view: 16.0,
    body: r#"<path d="M2.5 9V8A5.5 5.5 0 0 1 13.5 8V9" stroke="currentColor" stroke-linecap="round"/><rect x="2.25" y="9" width="3" height="4.25" rx="1.1" stroke="currentColor"/><rect x="10.75" y="9" width="3" height="4.25" rx="1.1" stroke="currentColor"/>"#,
};

static SLIDERS: Icon = Icon {
    name: "dex-sliders",
    view: 16.0,
    body: r#"<path d="M2.5 4.5H13.5M2.5 11.5H13.5" stroke="currentColor" stroke-linecap="round"/><circle cx="6" cy="4.5" r="1.6" fill="currentColor"/><circle cx="10.5" cy="11.5" r="1.6" fill="currentColor"/>"#,
};

static ARROW_LEFT: Icon = Icon {
    name: "dex-arrow-left",
    view: 16.0,
    body: r#"<path d="M7.5 3.5L3 8L7.5 12.5M3 8H13.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

static INFO: Icon = Icon {
    name: "dex-info",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6.1" stroke="currentColor"/><path d="M8 7.2V11M8 4.8V5.7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
};

// ── Theme and type ───────────────────────────────────────────────────────────

/// Graphite page, off-white ink, one warm tally accent. `from_palette` derives
/// the panel layers, secondary ink and semantic colours from these three.
fn kit() -> Kit {
    let mut k = Kit::from_palette_focus(
        PaletteSeed {
            bg: oklch(0.158, 0.006, 258.0),
            fg: oklch(0.955, 0.003, 258.0),
            accent: oklch(0.765, 0.105, 68.0),
        },
        PaletteFocus::Accent,
    );
    k.radius.sm = 5.0;
    k.radius.md = 8.0;
    k.radius.lg = 12.0;
    k.radius.xl = 14.0;
    k
}

fn display() -> TextStyle {
    TextStyle::new(27.0, 600.0).lh(33.0).tracking(-0.4)
}

fn title_style() -> TextStyle {
    TextStyle::new(14.0, 600.0).lh(20.0)
}

fn body_style() -> TextStyle {
    TextStyle::new(13.0, 400.0).lh(20.0)
}

fn reading_style() -> TextStyle {
    TextStyle::new(17.0, 400.0).lh(27.0)
}

fn label_style() -> TextStyle {
    TextStyle::new(11.5, 500.0).lh(16.0)
}

fn small_style() -> TextStyle {
    TextStyle::new(11.0, 400.0).lh(16.0)
}

fn faint_style() -> TextStyle {
    TextStyle::new(10.5, 400.0).lh(15.0)
}

fn counter_style() -> TextStyle {
    TextStyle::new(14.0, 500.0).lh(18.0).features("'tnum' 1")
}

// ── Status ───────────────────────────────────────────────────────────────────

struct StatusCopy {
    title: &'static str,
    chip: String,
    detail: String,
    color: Color,
}

fn failed_detail(engine_present: bool) -> &'static str {
    if engine_present {
        "The pipeline failed. Press Stop to release the engine, then retry or open Settings."
    } else {
        "The pipeline stopped. Fix the error below, then press Start again."
    }
}

fn status_copy(s: &State, k: &Kit) -> StatusCopy {
    if s.error.is_some() && s.status.state == EngineState::Idle {
        return StatusCopy {
            title: "Needs attention",
            chip: "Could not start".into(),
            detail: "The bridge did not start. Read the error below, then press Start again or open Settings."
                .into(),
            color: k.c.danger.fg,
        };
    }
    match s.status.state {
        EngineState::Idle if s.status.warning.as_deref() == Some("shutdown_incomplete") => {
            StatusCopy {
                title: "Stopped with a warning",
                chip: "Incomplete".into(),
                detail: "The last shutdown timed out. Audio is idle; start again when ready.".into(),
                color: k.c.warning.fg,
            }
        }
        EngineState::Idle => StatusCopy {
            title: "Ready",
            chip: "Idle".into(),
            detail: "Start the bridge to turn speech into audio on DexVoice.".into(),
            color: k.c.text_muted,
        },
        EngineState::Loading => StatusCopy {
            title: "Loading",
            chip: "Loading models".into(),
            detail: "Parakeet and Phonon are loading. Audio devices open when both are ready."
                .into(),
            color: k.c.text_accent,
        },
        EngineState::Running if s.status.input_paused => StatusCopy {
            title: "Speaking",
            chip: "Bridge live · mic muted".into(),
            detail: "Your microphone is muted while Dex Voice plays. Listening resumes after the quiet gap."
                .into(),
            color: k.c.text_accent,
        },
        EngineState::Running => StatusCopy {
            title: "Listening",
            chip: "Bridge live".into(),
            detail: "Speak normally. A phrase is sent after about 800 ms of silence.".into(),
            color: k.c.success.fg,
        },
        EngineState::Stopping => StatusCopy {
            title: "Stopping",
            chip: "Shutting down".into(),
            detail: "Finishing the current phrase and closing the audio devices.".into(),
            color: k.c.text_muted,
        },
        EngineState::Recovering => StatusCopy {
            title: "Reconnecting audio",
            chip: "Device recovering".into(),
            detail: "The output device stopped consuming audio. Dex Voice is reopening it once."
                .into(),
            color: k.c.warning.fg,
        },
        EngineState::Failed => match s.status.failure_domain {
            FailureDomain::Synthesis => StatusCopy {
                title: "Speech unavailable",
                chip: "Synthesis failed".into(),
                detail: "The speech synthesiser stopped responding or failed. Recognition is not the problem; restart the bridge to recover.".into(),
                color: k.c.danger.fg,
            },
            FailureDomain::Device => StatusCopy {
                title: "Audio device lost",
                chip: "Device failed".into(),
                detail: "The output device could not be recovered. Check the virtual microphone, then press Start again.".into(),
                color: k.c.danger.fg,
            },
            FailureDomain::None => StatusCopy {
                title: "Needs attention",
                chip: "Failed".into(),
                detail: failed_detail(s.engine.is_some()).into(),
                color: k.c.danger.fg,
            },
        },
    }
}

fn pipeline_active(s: &State) -> bool {
    s.engine.is_some()
        || matches!(
            s.status.state,
            EngineState::Loading | EngineState::Running | EngineState::Stopping
        )
}

fn error_message(s: &State) -> Option<String> {
    s.error.clone().or_else(|| s.status.error.clone())
}

fn output_label(s: &State) -> String {
    if s.settings.output_device.is_empty() {
        "Not selected".into()
    } else {
        device_label(&s.output_choices, &s.settings.output_device)
    }
}

fn tooltip_text(value: Option<f64>) -> String {
    value.map_or_else(|| "—".into(), |v| format!("{v:.0} ms"))
}

// ── Shared pieces ────────────────────────────────────────────────────────────

fn brand(k: &Kit) -> El {
    row()
        .gap(10.0)
        .center_y()
        .child(
            el().size(30.0, 30.0)
                .radius(9.0)
                .center()
                .fill(k.c.bg_accent.alpha(0.15))
                .border(1.0, k.c.bg_accent.alpha(0.28))
                .child(icon(&MIC, 17.0).color(k.c.text_accent).a11y_hidden()),
        )
        .child(
            col()
                .gap(1.0)
                .child(text("Dex Voice").font(title_style()).color(k.c.text_base))
                .child(
                    text("Local speech bridge")
                        .font(faint_style())
                        .color(k.c.text_muted),
                ),
        )
}

static MIC: Icon = Icon {
    name: "dex-mic",
    view: 16.0,
    body: r#"<rect x="5.5" y="1.5" width="5" height="8" rx="2.5" stroke="currentColor"/><path d="M3.5 7V7.5A4.5 4.5 0 0 0 12.5 7.5V7M8 12V14.5M5.5 14.5H10.5" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

fn header(s: &State, k: &Kit) -> El {
    let active = pipeline_active(s);
    let settings = Button::new("Settings")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .icon(&SLIDERS)
        .disabled(active)
        .help(if active {
            "Stop the bridge to change models, devices, or voice"
        } else {
            "Choose models, microphone, output, and voice"
        })
        .on_press(|s: &mut State| s.open_settings())
        .build(k)
        .key("settings");
    row()
        .w_full()
        .h(58.0)
        .px(22.0)
        .center_y()
        .justify_between()
        .child(brand(k))
        .child(settings)
}

fn status_chip(s: &State, k: &Kit, copy: &StatusCopy) -> El {
    let dot = if s.status.state == EngineState::Loading {
        Loader::new().size(13.0).build(k)
    } else {
        el().size(14.0, 14.0)
            .radius(7.0)
            .center()
            .fill(copy.color.alpha(0.16))
            .child(el().size(6.0, 6.0).radius(3.0).fill(copy.color))
    };
    row()
        .role("status")
        .label(copy.chip.clone())
        .gap(7.0)
        .center_y()
        .child(dot)
        .child(
            text(copy.chip.clone())
                .key("status-label")
                .font(label_style())
                .color(k.c.text_muted),
        )
}

fn hero(s: &State, k: &Kit) -> El {
    let copy = status_copy(s, k);
    let active = pipeline_active(s);
    let stopping = s.status.state == EngineState::Stopping;

    let start = Button::new("Start")
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::ExtraLarge)
        .icon(&PLAY)
        .disabled(active)
        .help(if active {
            "The bridge is already running"
        } else {
            "Load models and open the selected audio devices"
        })
        .on_press(|s: &mut State| s.start())
        .build(k)
        .key("start");
    let stop = Button::new("Stop")
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::ExtraLarge)
        .icon(&STOP)
        .disabled(!active || stopping)
        .help(if stopping {
            "The bridge is already stopping"
        } else if active {
            "Finish the current phrase and close the audio devices"
        } else {
            "The bridge is not running"
        })
        .on_press(|s: &mut State| s.stop())
        .build(k)
        .key("stop");

    let mut info = col()
        .grow(1.0)
        .shrink()
        .gap(5.0)
        .child(status_chip(s, k, &copy))
        .child(
            text(copy.title)
                .key("state-title")
                .font(display())
                .color(k.c.text_base),
        )
        .child(
            text(copy.detail)
                .key("state-detail")
                .font(body_style())
                .color(k.c.text_muted)
                .wrap()
                .max_w(520.0),
        );
    if let Some(message) = error_message(s) {
        info = info.child(
            row()
                .role("alert")
                .gap(7.0)
                .items_start()
                .child(icon(&INFO, 13.0).color(k.c.danger.fg).mt(2.0).a11y_hidden())
                .child(
                    text(message)
                        .key("error-text")
                        .font(small_style())
                        .color(k.c.danger.fg)
                        .wrap()
                        .max_w(520.0),
                ),
        );
    }

    row()
        .w_full()
        .padding(20.0)
        .gap(24.0)
        .items_start()
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(14.0)
        .child(info)
        .child(row().gap(8.0).center_y().child(start).child(stop))
}

fn alert(k: &Kit, message: &str) -> El {
    row()
        .w_full()
        .padding(11.0)
        .gap(9.0)
        .center_y()
        .radius(9.0)
        .fill(k.c.danger.bg)
        .border(1.0, k.c.danger.border)
        .role("alert")
        .child(icon(&INFO, 14.0).color(k.c.danger.fg).a11y_hidden())
        .child(
            text(message)
                .font(small_style())
                .color(k.c.danger.fg)
                .wrap()
                .grow(1.0),
        )
}

fn panel(k: &Kit) -> El {
    col()
        .w_full()
        .padding(16.0)
        .gap(12.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(12.0)
}

fn panel_heading(k: &Kit, title: &str) -> El {
    row()
        .w_full()
        .center_y()
        .justify_between()
        .child(text(title).font(label_style()).color(k.c.text_muted))
}

fn transcript_card(s: &State, k: &Kit) -> El {
    let has_text = !s.status.transcript.trim().is_empty();
    let clear = Button::new("Clear")
        .variant(ButtonVariant::GhostMuted)
        .control_size(ControlSize::Regular)
        .disabled(!has_text)
        .help(if has_text {
            "Clear the transcript without stopping recognition"
        } else {
            "Nothing to clear yet"
        })
        .on_press(|s: &mut State| {
            if let Some(engine) = &s.engine {
                engine.clear_transcript();
            }
            s.status.transcript.clear();
            s.status.words = 0;
        })
        .build(k)
        .key("clear");

    let words = format!(
        "{} word{}",
        s.status.words,
        if s.status.words == 1 { "" } else { "s" }
    );

    let body = if has_text {
        el().w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: true })
            .child(
                text(s.status.transcript.clone())
                    .key("transcript-text")
                    .font(reading_style())
                    .color(k.c.text_base)
                    .wrap()
                    .w_full()
                    .selectable(true),
            )
    } else if s.status.state == EngineState::Loading {
        row()
            .grow(1.0)
            .min_h(70.0)
            .center()
            .gap(12.0)
            .child(Loader::new().size(20.0).build(k))
            .child(
                col()
                    .gap(2.0)
                    .items_start()
                    .child(
                        text("Loading Parakeet and Phonon…")
                            .font(body_style())
                            .color(k.c.text_muted),
                    )
                    .child(
                        text("The transcript starts when the models are ready.")
                            .font(small_style())
                            .color(k.c.text_muted),
                    ),
            )
    } else {
        let (heading, detail) = if s.status.state == EngineState::Failed {
            (
                "No transcript yet",
                "Fix the error above, then start the bridge again.",
            )
        } else if s.status.state == EngineState::Running {
            (
                "Waiting for speech",
                "Speak a phrase; recognized words appear after the pause.",
            )
        } else if s.status.state == EngineState::Stopping {
            (
                "Finishing up",
                "The transcript closes after the current phrase finishes.",
            )
        } else {
            (
                "No speech yet",
                "Start the bridge, then speak. Recognized words appear here.",
            )
        };
        row()
            .grow(1.0)
            .min_h(70.0)
            .center()
            .gap(12.0)
            .child(
                el().size(40.0, 40.0)
                    .radius(11.0)
                    .center()
                    .fill(k.c.bg_accent.alpha(0.12))
                    .child(icon(&MIC, 19.0).color(k.c.text_accent).a11y_hidden()),
            )
            .child(
                col()
                    .gap(2.0)
                    .items_start()
                    .child(text(heading).font(body_style()).color(k.c.text_muted))
                    .child(
                        text(detail)
                            .font(small_style())
                            .color(k.c.text_muted)
                            .wrap()
                            .max_w(340.0),
                    ),
            )
    };

    col()
        .grow(1.0)
        .shrink()
        .min_w(360.0)
        .role("region")
        .label("Transcript")
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(14.0)
        .child(
            row()
                .w_full()
                .h(48.0)
                .px(16.0)
                .center_y()
                .gap(10.0)
                .child(text("Transcript").font(title_style()).color(k.c.text_base))
                .child(
                    text(words)
                        .key("word-count")
                        .font(small_style())
                        .color(k.c.text_muted),
                )
                .child(el().grow(1.0))
                .child(clear),
        )
        .child(el().w_full().h(1.0).fill(k.c.border_base))
        .child(el().w_full().grow(1.0).min_h(0.0).padding(14.0).child(body))
}

fn route_step(k: &Kit, glyph: &'static Icon, label: &str, value: &str, tag: &str) -> El {
    let description = format!("{label}: {value}");
    row()
        .w_full()
        .role("group")
        .label(description.clone())
        .help(description)
        .gap(10.0)
        .center_y()
        .child(
            el().size(28.0, 28.0)
                .radius(8.0)
                .center()
                .fill(k.c.layer_02)
                .child(icon(glyph, 15.0).color(k.c.icon_muted).a11y_hidden()),
        )
        .child(
            col()
                .grow(1.0)
                .shrink()
                .gap(1.0)
                .child(text(label).font(faint_style()).color(k.c.text_muted))
                .child(
                    text(value.to_owned())
                        .font(TextStyle::new(12.0, 500.0).lh(17.0))
                        .color(k.c.text_base)
                        .truncate(),
                ),
        )
        .child(
            text(tag)
                .font(TextStyle::new(9.5, 500.0).lh(12.0).tracking(0.35))
                .color(k.c.text_muted),
        )
}

fn connector(k: &Kit) -> El {
    el().w_full()
        .h(10.0)
        .padding(0.0)
        .child(el().w(1.0).h_full().ml(13.5).fill(k.c.border_base))
}

fn routing_card(s: &State, k: &Kit) -> El {
    let output = output_label(s);
    let missing_output = s.settings.output_device.is_empty();
    let unlisted_output = s.output_choices.is_empty();
    let unavailable_output = !missing_output
        && !unlisted_output
        && !s
            .output_choices
            .iter()
            .any(|choice| choice.id == s.settings.output_device);

    let warning = !pipeline_active(s) && (missing_output || unlisted_output || unavailable_output);
    let mut card = panel(k)
        .role("group")
        .label("Routing")
        .child(panel_heading(k, "Routing"))
        .child(route_step(
            k,
            &MIC,
            "Microphone",
            &device_label(&s.input_choices, &s.settings.input_device),
            "INPUT",
        ))
        .child(connector(k))
        .child(route_step(k, &SPEAKER, "DexVoice output", &output, "SINK"))
        .child(connector(k))
        .child(route_step(
            k,
            &HEADPHONES,
            "Receiving app",
            "DexMicrophone",
            "EXPECTED",
        ))
        .child(
            text(
                "Expected, not detected: choose DexMicrophone as the microphone in the other app.",
            )
            .font(faint_style())
            .color(k.c.text_muted)
            .wrap()
            .w_full(),
        );

    if warning {
        card = card.child(
            text(if missing_output {
                "Choose a virtual output in Settings before starting."
            } else if unlisted_output {
                "No output devices are listed. Open Settings and refresh."
            } else {
                "The selected output is disconnected. Choose an available virtual sink."
            })
            .font(small_style())
            .color(k.c.warning.fg)
            .wrap()
            .w_full(),
        );
    } else {
        card = card
            .child(el().w_full().h(1.0).fill(k.c.border_base))
            .child(
                text("Half-duplex echo guard: the microphone mutes while Dex Voice plays.")
                    .font(faint_style())
                    .color(k.c.text_muted)
                    .wrap()
                    .w_full(),
            );
    }
    card
}

fn signal_cell(k: &Kit, label: &str, hint: &str, value: &str) -> El {
    col()
        .grow(1.0)
        .shrink()
        .gap(1.0)
        .child(
            text(value.to_owned())
                .font(counter_style())
                .color(k.c.text_base),
        )
        .child(
            text(label)
                .font(TextStyle::new(10.5, 500.0).lh(13.0))
                .color(k.c.text_muted)
                .wrap()
                .w_full(),
        )
        .child(
            text(hint.to_owned())
                .font(TextStyle::new(9.5, 400.0).lh(12.0))
                .color(k.c.text_muted)
                .wrap()
                .w_full(),
        )
}

fn signal_bar(s: &State, k: &Kit) -> El {
    let queue = format!("{:.1} s", s.status.queued_seconds);
    let dropped = format!("{:.1} s", s.status.dropped_seconds);
    let mic_drops = (s.status.input_drops + s.status.capture_drops).to_string();
    col()
        .w_full()
        .padding(12.0)
        .gap(8.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(12.0)
        .role("group")
        .label("Signal")
        .child(text("Signal").font(label_style()).color(k.c.text_muted))
        .child(
            text("p50/p95: first hypothesis (committed or tentative) → first queued PCM (includes the phrase wait and synthesis; not playback or acoustic end-to-end latency).")
                .font(faint_style())
                .color(k.c.text_muted)
                .wrap()
                .w_full(),
        )
        .child(
            row()
                .w_full()
                .gap(12.0)
                .items_start()
                .child(signal_cell(
                    k,
                    "p50",
                    "word → PCM",
                    &tooltip_text(s.status.p50_ms),
                ))
                .child(signal_cell(
                    k,
                    "p95",
                    "95th percentile",
                    &tooltip_text(s.status.p95_ms),
                ))
                .child(signal_cell(k, "Queue", "waiting", &queue))
                .child(signal_cell(k, "Dropped", "output", &dropped))
                .child(signal_cell(k, "Mic drops", "input+capture", &mic_drops)),
        )
}

fn footer(k: &Kit) -> El {
    row()
        .w_full()
        .gap(7.0)
        .items_start()
        .child(
            icon(&INFO, 13.0)
                .color(k.c.text_muted)
                .mt(1.0)
                .a11y_hidden(),
        )
        .child(
            text("Linux preview · experimental · half-duplex · phrases send after ~800 ms of silence (10 s max) · latency above 500 ms.")
                .font(faint_style())
                .color(k.c.text_muted)
                .wrap()
                .grow(1.0),
        )
}

fn main_body(s: &State, k: &Kit) -> El {
    let inner = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .padding(20.0)
        .gap(14.0)
        .child(hero(s, k));

    let inner = inner
        .child(
            row()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .gap(14.0)
                .items_stretch()
                .child(
                    col()
                        .grow(1.0)
                        .shrink()
                        .min_w(360.0)
                        .h_full()
                        .min_h(0.0)
                        .gap(12.0)
                        .child(transcript_card(s, k))
                        .child(signal_bar(s, k)),
                )
                .child(
                    col()
                        .w(296.0)
                        .h_full()
                        .min_h(0.0)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .child(col().w_full().child(routing_card(s, k))),
                ),
        )
        .child(footer(k));

    col()
        .w_full()
        .h_full()
        .fill(k.c.page)
        .child(header(s, k))
        .child(el().w_full().h(1.0).fill(k.c.border_base))
        .child(inner)
}

// ── Settings ─────────────────────────────────────────────────────────────────

fn settings_header(k: &Kit) -> El {
    let back = IconButton::new(&ARROW_LEFT)
        .variant(IconButtonVariant::GhostMuted)
        .control_size(ControlSize::Large)
        .label("Back")
        .help("Back to the console")
        .on_press(|s: &mut State| s.close_settings())
        .build(k)
        .key("back");
    row()
        .w_full()
        .h(58.0)
        .px(22.0)
        .gap(10.0)
        .center_y()
        .child(back)
        .child(
            col()
                .gap(1.0)
                .child(text("Settings").font(title_style()).color(k.c.text_base))
                .child(
                    text("Changes apply the next time you start the bridge.")
                        .font(faint_style())
                        .color(k.c.text_muted),
                ),
        )
}

fn settings_card(k: &Kit, title: &str, description: &str, trailing: Option<El>) -> El {
    let mut heading = row().w_full().center_y().gap(10.0).child(
        col()
            .grow(1.0)
            .shrink()
            .gap(2.0)
            .child(
                text(title)
                    .font(TextStyle::new(13.5, 600.0).lh(19.0))
                    .color(k.c.text_base),
            )
            .child(
                text(description.to_owned())
                    .font(faint_style())
                    .color(k.c.text_muted)
                    .wrap(),
            ),
    );
    if let Some(trailing) = trailing {
        heading = heading.child(trailing);
    }
    col()
        .w_full()
        .padding(18.0)
        .gap(14.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(12.0)
        .child(heading)
}

fn labeled_control(k: &Kit, label: &str, hint: &str, control: El) -> El {
    col()
        .w_full()
        .gap(6.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .items_start()
                .child(
                    text(label)
                        .font(TextStyle::new(12.0, 500.0).lh(17.0))
                        .color(k.c.text_base),
                )
                .child(
                    text(hint.to_owned())
                        .font(faint_style())
                        .color(k.c.text_muted)
                        .wrap()
                        .grow(1.0),
                ),
        )
        .child(control)
}

fn models_card(s: &State, k: &Kit) -> El {
    let asr = TextInput::new(&s.asr_edit)
        .width(Some(660.0))
        .height(34.0)
        .label("Parakeet GGUF")
        .on_edit(|s: &mut State| &mut s.asr_edit)
        .build(k)
        .key("asr-path");
    let tts = TextInput::new(&s.tts_edit)
        .width(Some(660.0))
        .height(34.0)
        .label("Phonon checkpoint")
        .on_edit(|s: &mut State| &mut s.tts_edit)
        .build(k)
        .key("tts-path");
    settings_card(
        k,
        "Models",
        "Local files only. This window never downloads models.",
        None,
    )
    .child(labeled_control(
        k,
        "Parakeet GGUF",
        "Parakeet Unified English, F16.",
        asr,
    ))
    .child(labeled_control(
        k,
        "Phonon checkpoint",
        "Folder containing config.json and the installed voices.",
        tts,
    ))
}

fn audio_card(s: &State, k: &Kit, layer: &mut Vec<El>) -> El {
    let refresh = Button::new("Refresh")
        .variant(ButtonVariant::GhostMuted)
        .control_size(ControlSize::Small)
        .help("Re-list inputs and outputs. System defaults are not changed.")
        .on_press(|s: &mut State| s.refresh_devices())
        .build(k)
        .key("refresh-devices");

    let mut input_labels = vec!["System default".to_owned()];
    input_labels.extend(device_labels(&s.input_choices));
    let input_current = device_label(&s.input_choices, &s.draft.input_device);
    let input_select = Select::new("select-input", &input_labels)
        .appearance(SelectAppearance::Field)
        .width(350.0)
        .label("Microphone")
        .current(Some(input_current.as_str()))
        .open(s.open_select == Some(SelectMenu::Input))
        .on_toggle(|s: &mut State| {
            s.open_select = if s.open_select == Some(SelectMenu::Input) {
                None
            } else {
                Some(SelectMenu::Input)
            }
        })
        .on_close(|s: &mut State| s.open_select = None)
        .on_choose(|s: &mut State, value: &str, _: &mut Tx| s.choose_device(true, value))
        .build(k, layer)
        .key("select-input-trigger");

    let output_labels: Vec<String> = device_labels(&s.output_choices);
    let output_empty = output_labels.is_empty();
    let output_current = if s.draft.output_device.is_empty() {
        None
    } else {
        Some(device_label(&s.output_choices, &s.draft.output_device))
    };
    let output_select = Select::new("select-output", &output_labels)
        .appearance(SelectAppearance::Field)
        .width(350.0)
        .label("Virtual microphone output")
        .placeholder("Choose DexVoice or another virtual sink")
        .current(output_current.as_deref())
        .invalid(s.draft.output_device.is_empty())
        .disabled(output_empty)
        .open(s.open_select == Some(SelectMenu::Output))
        .on_toggle(|s: &mut State| {
            s.open_select = if s.open_select == Some(SelectMenu::Output) {
                None
            } else {
                Some(SelectMenu::Output)
            }
        })
        .on_close(|s: &mut State| s.open_select = None)
        .on_choose(|s: &mut State, value: &str, _: &mut Tx| s.choose_device(false, value))
        .build(k, layer)
        .key("select-output-trigger");

    let mut card = settings_card(
        k,
        "Audio routing",
        "Explicit devices. The app never changes your system defaults.",
        Some(refresh),
    )
    .child(labeled_control(
        k,
        "Microphone",
        "Capture input. System default is allowed.",
        input_select,
    ))
    .child(labeled_control(
        k,
        "Virtual microphone output",
        "Virtual sink (DexVoice). Never choose speakers.",
        output_select,
    ));

    if let Some(error) = &s.device_error {
        card = card.child(alert(k, error));
    } else if s.input_choices.is_empty() || s.output_choices.is_empty() {
        let message = match (s.input_choices.is_empty(), s.output_choices.is_empty()) {
            (true, true) => {
                "No input or output devices were detected. Refresh after connecting them."
            }
            (true, false) => {
                "No input devices were detected. Refresh after connecting a microphone."
            }
            (false, true) => "No output devices were detected. Create DexVoice, then refresh.",
            (false, false) => "",
        };
        if !message.is_empty() {
            card = card.child(
                text(message)
                    .font(small_style())
                    .color(k.c.warning.fg)
                    .wrap(),
            );
        }
    }

    card
}

fn voice_card(s: &State, k: &Kit, layer: &mut Vec<El>) -> El {
    let voices = config::available_voices(&PathBuf::from(s.tts_edit.text.trim()));
    let labels = voice_labels(&voices);
    let current = voices
        .iter()
        .position(|id| *id == s.draft.voice)
        .map(|index| labels[index].clone());
    let select = Select::new("select-voice", &labels)
        .appearance(SelectAppearance::Field)
        .width(250.0)
        .label("Voice")
        .placeholder(if voices.is_empty() {
            "No installed voice embeddings"
        } else {
            "Choose an installed voice"
        })
        .current(current.as_deref())
        .disabled(voices.is_empty())
        .open(s.open_select == Some(SelectMenu::Voice))
        .on_toggle(|s: &mut State| {
            s.open_select = if s.open_select == Some(SelectMenu::Voice) {
                None
            } else {
                Some(SelectMenu::Voice)
            }
        })
        .on_close(|s: &mut State| s.open_select = None)
        .on_choose(|s: &mut State, value: &str, _: &mut Tx| s.choose_voice(value))
        .build(k, layer)
        .key("select-voice-trigger");

    let mut card = settings_card(
        k,
        "Voice",
        "Only embeddings installed beside the selected checkpoint are listed.",
        None,
    )
    .child(labeled_control(
        k,
        "Phonon voice",
        if voices.is_empty() {
            "No default-voice.safetensors or voices/*.safetensors found."
        } else {
            "Default, Zoey, and Juniper ship with the Linux model script."
        },
        select,
    ));
    if voices.is_empty() {
        card = card.child(
            text("Install a voice embedding in the checkpoint's voices folder, then reopen Settings.")
                .font(small_style())
                .color(k.c.warning.fg)
                .wrap(),
        );
    }
    card
}

fn settings_body(s: &State, k: &Kit, layer: &mut Vec<El>) -> El {
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .on_press(|s: &mut State| s.close_settings())
        .build(k)
        .key("cancel-settings");
    let save = Button::new("Save changes")
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Regular)
        .disabled(!s.draft_dirty())
        .help(if s.draft_dirty() {
            "Validate and save these settings"
        } else {
            "No changes to save"
        })
        .on_press(|s: &mut State| s.save_settings())
        .build(k)
        .key("save-settings");

    let content = col()
        .w_full()
        .max_w(720.0)
        .gap(14.0)
        .child(models_card(s, k))
        .child(
            row()
                .w_full()
                .gap(14.0)
                .items_start()
                .child(
                    col()
                        .grow(1.0)
                        .shrink()
                        .min_w(0.0)
                        .child(audio_card(s, k, layer)),
                )
                .child(col().w(300.0).shrink().child(voice_card(s, k, layer))),
        );

    let mut footer = col().w_full().padding(16.0).gap(10.0);
    if let Some(message) = error_message(s) {
        footer = footer.child(alert(k, &message));
    }
    footer = footer.child(
        row()
            .w_full()
            .center_y()
            .child(
                text("Settings are stored locally and restored on launch.")
                    .font(faint_style())
                    .color(k.c.text_muted),
            )
            .child(el().grow(1.0))
            .child(cancel)
            .child(el().w(8.0))
            .child(save),
    );

    col()
        .w_full()
        .h_full()
        .fill(k.c.page)
        .child(settings_header(k))
        .child(el().w_full().h(1.0).fill(k.c.border_base))
        .child(
            el().w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(col().w_full().padding(22.0).items_center().child(content)),
        )
        .child(el().w_full().h(1.0).fill(k.c.border_base))
        .child(footer)
}

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn view(s: &State, _: &mut Cx<State>) -> El {
    let k = kit();
    let mut layer = Vec::new();
    let body = if s.settings_open {
        settings_body(s, &k, &mut layer)
    } else {
        main_body(s, &k)
    };
    rune_kit::root(body, layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dex_audio::DeviceChoice;
    use rune::{Env, Runtime};

    fn runtime(state: State, size: (f32, f32)) -> Runtime<State> {
        let mut rt = Runtime::new(state, view, size, Env::new(true, 1.0));
        rt.frame(0.0);
        rt
    }

    fn body_text(rt: &Runtime<State>) -> String {
        rt.elements()
            .iter()
            .filter_map(|element| element.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn focusable(rt: &Runtime<State>, key: &str) -> bool {
        rt.elements()
            .iter()
            .find(|element| element.key.as_deref() == Some(key))
            .is_some_and(|element| element.focusable)
    }

    fn settings_fixture() -> State {
        let mut state = State::test_fixture();
        state.settings_open = true;
        state.draft.output_device = "dexvoice".into();
        state.settings.output_device = "dexvoice".into();
        state.input_choices = vec![DeviceChoice {
            id: "jade".into(),
            label: "JadeAudio JA11 Mono".into(),
        }];
        state.output_choices = vec![DeviceChoice {
            id: "dexvoice".into(),
            label: "DexVoice".into(),
        }];
        state.draft.input_device = "jade".into();
        state
    }

    #[test]
    fn idle_console_shows_ready_state_and_routing_hint() {
        let rt = runtime(State::test_fixture(), (900.0, 640.0));
        let text = body_text(&rt);
        assert!(text.contains("Ready"));
        assert!(text.contains("Idle"));
        assert!(text.contains("DexMicrophone"));
        assert!(text.contains("Start"));
        assert!(rt.find("start").is_some());
        assert!(rt.find("settings").is_some());
        assert!(focusable(&rt, "start"));
    }

    #[test]
    fn running_console_offers_stop_and_real_counters() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Running;
        state.status.transcript = "Well, I don't wish to see it any more.".into();
        state.status.words = 9;
        state.status.p50_ms = Some(281.0);
        state.status.p95_ms = Some(684.0);
        let rt = runtime(state, (900.0, 640.0));
        let text = body_text(&rt);
        assert!(text.contains("Listening"));
        assert!(text.contains("Well, I don't wish to see it any more."));
        assert!(text.contains("281 ms"));
        assert!(text.contains("684 ms"));
        assert!(focusable(&rt, "stop"));
        assert!(!focusable(&rt, "start"));
        let clear = rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("clear"))
            .expect("clear button");
        assert!(clear.focusable);
    }

    #[test]
    fn speaking_state_says_the_microphone_is_muted() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Running;
        state.status.input_paused = true;
        let rt = runtime(state, (900.0, 640.0));
        let text = body_text(&rt);
        assert!(text.contains("Speaking"));
        assert!(text.contains("mic muted"));
    }

    #[test]
    fn failed_state_shows_an_inline_error_banner() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Failed;
        state.status.error = Some("no GPU matching \"nvidia\"".into());
        let rt = runtime(state, (900.0, 640.0));
        let banner = rt
            .elements()
            .into_iter()
            .find(|element| element.role == "alert")
            .expect("error banner");
        assert_eq!(banner.label.as_deref(), None);
        assert!(body_text(&rt).contains("no GPU matching"));
    }

    #[test]
    fn synthesis_failure_is_not_reported_as_a_device_failure() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Failed;
        state.status.failure_domain = FailureDomain::Synthesis;
        state.status.synthesis_state = dex_core::SynthesisState::Stalled;
        let text = body_text(&runtime(state, (900.0, 640.0)));
        assert!(text.contains("Speech unavailable"));
        assert!(text.contains("Synthesis failed"));
        assert!(!text.contains("Audio device lost"));
    }

    #[test]
    fn device_failure_is_not_reported_as_a_synthesis_failure() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Failed;
        state.status.failure_domain = FailureDomain::Device;
        state.status.device_state = dex_core::DeviceState::Lost;
        let text = body_text(&runtime(state, (900.0, 640.0)));
        assert!(text.contains("Audio device lost"));
        assert!(text.contains("Device failed"));
        assert!(!text.contains("Speech unavailable"));
    }

    #[test]
    fn recovering_state_says_the_device_is_reopening() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Recovering;
        let text = body_text(&runtime(state, (900.0, 640.0)));
        assert!(text.contains("Reconnecting audio"));
        assert!(text.contains("Device recovering"));
    }

    #[test]
    fn incomplete_shutdown_is_visible_in_idle() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Idle;
        state.status.warning = Some("shutdown_incomplete".into());
        let text = body_text(&runtime(state, (900.0, 640.0)));
        assert!(text.contains("Stopped with a warning"));
        assert!(text.contains("Incomplete"));
    }

    #[test]
    fn settings_exposes_explicit_selects_and_back_navigation() {
        let rt = runtime(settings_fixture(), (900.0, 640.0));
        assert!(rt.find("select-input-trigger").is_some());
        assert!(rt.find("select-output-trigger").is_some());
        assert!(rt.find("select-voice-trigger").is_some());
        assert!(rt.find("refresh-devices").is_some());
        assert!(rt.find("back").is_some());
        assert!(rt.find("asr-path").is_some());
        assert!(rt.find("tts-path").is_some());
        assert!(focusable(&rt, "save-settings"));
    }

    #[test]
    fn settings_fields_have_programmatic_labels() {
        let rt = runtime(settings_fixture(), (900.0, 640.0));
        let labels: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|element| element.role == "textbox")
            .filter_map(|element| element.label)
            .collect();
        assert!(labels.iter().any(|label| label == "Parakeet GGUF"));
        assert!(labels.iter().any(|label| label == "Phonon checkpoint"));
    }

    #[test]
    fn keyboard_tab_reaches_start_and_skips_disabled_controls() {
        use rune::{Key, KeyEvent, Mods};

        let mut rt = runtime(State::test_fixture(), (900.0, 640.0));
        let mut reached = Vec::new();
        for _ in 0..6 {
            rt.key(KeyEvent {
                key: Key::Tab,
                mods: Mods::default(),
            });
            rt.frame(0.0);
            if let Some(key) = rt
                .elements()
                .into_iter()
                .find(|element| element.focused)
                .and_then(|element| element.key)
            {
                reached.push(key);
            }
        }
        assert!(reached.iter().any(|key| key == "start"), "{reached:?}");
        assert!(!reached.iter().any(|key| key == "stop"), "{reached:?}");
        assert!(!reached.iter().any(|key| key == "clear"), "{reached:?}");
        let start = rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("start"))
            .expect("start button");
        assert!(start.focus_visible, "keyboard focus should show a ring");
    }

    #[test]
    fn escape_closes_the_select_without_leaving_settings() {
        use rune::{Key, KeyEvent, Mods};

        let mut rt = runtime(settings_fixture(), (900.0, 640.0));
        let (x, y) = rt.find("select-input-trigger").expect("input select");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.open_select, Some(SelectMenu::Input));
        rt.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        rt.frame(0.0);
        assert_eq!(rt.state.open_select, None);
        assert!(rt.state.settings_open);
    }

    #[test]
    fn back_button_returns_to_the_console() {
        let mut rt = runtime(settings_fixture(), (900.0, 640.0));
        let (x, y) = rt.find("back").expect("back button");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(!rt.state.settings_open);
        assert!(rt.find("start").is_some());
    }

    #[test]
    fn local_start_error_is_needs_attention_with_retry_enabled() {
        let mut state = State::test_fixture();
        state.error = Some("Select the virtual microphone output in Settings.".into());
        let rt = runtime(state, (900.0, 640.0));
        let text = body_text(&rt);
        assert!(text.contains("Needs attention"));
        assert!(text.contains("Could not start"));
        assert!(text.contains("Select the virtual microphone output"));
        assert!(focusable(&rt, "start"));
        assert!(!focusable(&rt, "stop"));
    }

    #[test]
    fn failed_copy_distinguishes_attached_engine_from_released_engine() {
        assert!(failed_detail(true).contains("Press Stop"));
        assert!(failed_detail(false).contains("press Start"));
        assert!(!failed_detail(false).contains("Press Stop"));
    }

    #[test]
    fn stopping_disables_the_duplicate_stop_action() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Stopping;
        let rt = runtime(state, (900.0, 640.0));
        assert!(body_text(&rt).contains("Shutting down"));
        assert!(!focusable(&rt, "stop"));
        assert!(!focusable(&rt, "start"));
    }

    #[test]
    fn metrics_name_the_commit_to_queued_pcm_boundary() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Running;
        state.status.p50_ms = Some(281.0);
        state.status.p95_ms = Some(684.0);
        let rt = runtime(state, (900.0, 640.0));
        let text = body_text(&rt);
        // Exact approved wording: first hypothesis (committed or tentative)
        // to first queued PCM (code measures first push, not end of
        // synthesis). Old "first committed word" / "(incl. synthesis)" must be gone.
        assert!(text.contains("p50/p95: first hypothesis (committed or tentative) → first queued PCM (includes the phrase wait and synthesis; not playback or acoustic end-to-end latency)."));
        assert!(!text.contains("first committed word"));
        assert!(!text.contains("(incl. synthesis)"));
        assert!(text.contains("95th percentile"));
        assert!(!text.contains("slowest 5%"));
    }

    #[test]
    fn mic_drops_include_capture_loss() {
        let mut state = State::test_fixture();
        state.status.state = EngineState::Running;
        state.status.input_drops = 7;
        state.status.capture_drops = 11;
        let rt = runtime(state, (900.0, 640.0));
        let texts: Vec<String> = rt
            .elements()
            .iter()
            .filter_map(|element| element.text.clone())
            .collect();
        // Specific cell value: an exact "18" text node (7 + 11), not merely a
        // substring that could match "180" or nearby counters.
        assert!(texts.iter().any(|t| t == "18"), "{texts:?}");
        assert!(
            !texts.iter().any(|t| t != "18" && t.contains("18")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t == "Mic drops"), "{texts:?}");
        assert!(texts.iter().any(|t| t == "input+capture"), "{texts:?}");
        // Sum, not a constant: a different total changes the cell.
        let mut state = State::test_fixture();
        state.status.state = EngineState::Running;
        state.status.input_drops = 3;
        state.status.capture_drops = 4;
        let rt = runtime(state, (900.0, 640.0));
        let texts: Vec<String> = rt
            .elements()
            .iter()
            .filter_map(|element| element.text.clone())
            .collect();
        assert!(texts.iter().any(|t| t == "7"), "{texts:?}");
        assert!(!texts.iter().any(|t| t == "18"), "{texts:?}");
    }

    #[test]
    fn receiving_input_is_presented_as_expected_not_confirmed() {
        let rt = runtime(State::test_fixture(), (900.0, 640.0));
        assert!(body_text(&rt).contains("Expected, not detected"));
    }

    #[test]
    fn long_device_names_keep_a_full_accessible_label() {
        let mut state = State::test_fixture();
        state.input_choices = vec![DeviceChoice {
            id: "long".into(),
            label: "Studio Booth Microphone With A Very Long Descriptive Name".into(),
        }];
        state.settings.input_device = "long".into();
        let rt = runtime(state, (760.0, 600.0));
        let row = rt
            .elements()
            .into_iter()
            .find(|element| {
                element
                    .label
                    .as_deref()
                    .is_some_and(|label| label.starts_with("Microphone: "))
            })
            .expect("microphone route row");
        assert_eq!(
            row.label.as_deref(),
            Some("Microphone: Studio Booth Microphone With A Very Long Descriptive Name")
        );
        assert_eq!(
            row.help.as_deref(),
            Some("Microphone: Studio Booth Microphone With A Very Long Descriptive Name")
        );
    }

    #[test]
    fn choosing_a_microphone_updates_the_draft_once() {
        let mut rt = runtime(settings_fixture(), (900.0, 640.0));
        let (x, y) = rt.find("select-input-trigger").expect("input select");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        let (x, y) = rt.find("option1").expect("JadeAudio option");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.draft.input_device, "jade");
        assert_eq!(rt.state.open_select, None);
    }
}
