"use client";

import { useEffect, useRef, useState } from "react";
import { Check, Copy } from "@phosphor-icons/react";

type CopyStatus = "idle" | "copied" | "error";

type CopyButtonProps = {
  value: string;
  label?: string;
  className?: string;
};

function fallbackCopy(text: string) {
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  document.body.removeChild(textarea);
  if (!copied) throw new Error("Copy command was rejected");
}

export function CopyButton({ value, label = "Copy", className = "" }: CopyButtonProps) {
  const [status, setStatus] = useState<CopyStatus>("idle");
  const resetTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (resetTimer.current) clearTimeout(resetTimer.current);
    };
  }, []);

  const scheduleReset = () => {
    if (resetTimer.current) clearTimeout(resetTimer.current);
    resetTimer.current = setTimeout(() => setStatus("idle"), 1800);
  };

  const copy = async () => {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(value);
      } else {
        fallbackCopy(value);
      }
      setStatus("copied");
    } catch {
      try {
        fallbackCopy(value);
        setStatus("copied");
      } catch {
        setStatus("error");
      }
    }
    scheduleReset();
  };

  const visibleLabel = status === "copied" ? "Copied" : status === "error" ? "Copy failed" : label;
  const accessibleLabel =
    status === "copied"
      ? `Copied to clipboard: ${value}`
      : status === "error"
        ? `Copy failed: ${value}`
        : `${label}: ${value}`;

  return (
    <button
      type="button"
      onClick={copy}
      data-copied={status === "copied"}
      className={`copy-button inline-flex min-h-10 shrink-0 items-center justify-center gap-1.5 rounded-md border border-line px-2.5 py-1.5 font-mono text-[9px] uppercase tracking-[0.08em] text-muted transition-colors hover:border-line-strong hover:text-bone ${className}`}
      aria-label={accessibleLabel}
    >
      {status === "copied" ? (
        <Check size={12} weight="bold" aria-hidden="true" />
      ) : (
        <Copy size={12} weight="bold" aria-hidden="true" />
      )}
      <span>{visibleLabel}</span>
      <span className="sr-only" aria-live="polite">
        {status === "copied"
          ? "Command copied to clipboard."
          : status === "error"
            ? "Copy failed. Select the command and copy it manually."
            : ""}
      </span>
    </button>
  );
}
