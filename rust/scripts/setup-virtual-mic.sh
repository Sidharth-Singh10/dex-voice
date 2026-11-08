#!/usr/bin/env bash
# Idempotently create the virtual microphone used by Dex Voice.
#
# The app writes synthesized speech into a null sink, and its monitor is
# re-exposed as a normal microphone. A plain monitor is hidden by many
# applications (Discord, some browsers), while a remapped source is listed
# like any other microphone.
set -euo pipefail

sink=dex_voice
source=dex_mic

has_sink() { pactl list short sinks | cut -f2 | grep -qx "$sink"; }
has_source() { pactl list short sources | cut -f2 | grep -qx "$source"; }

if has_sink; then
    printf 'Sink already present: %s\n' "$sink"
else
    pactl load-module module-null-sink sink_name="$sink" \
        sink_properties=device.description=DexVoice >/dev/null
    printf 'Created sink: %s\n' "$sink"
fi

if has_source; then
    printf 'Microphone already present: %s\n' "$source"
else
    # A description cannot contain spaces: pactl splits module arguments on
    # whitespace and would silently truncate it.
    pactl load-module module-remap-source master="$sink.monitor" \
        source_name="$source" source_properties=device.description=DexMicrophone >/dev/null
    printf 'Created virtual microphone: %s\n' "$source"
fi

printf 'App output: DexVoice. Call apps microphone: "DexMicrophone".\n'
