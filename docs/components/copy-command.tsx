"use client";

import { useId, useState } from "react";
import styles from "./copy-command.module.css";

function CopyIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <rect
        x="5.5"
        y="5.5"
        width="8"
        height="8"
        rx="1.25"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.25"
      />
      <path
        d="M10.5 5.5V3.75A1.25 1.25 0 0 0 9.25 2.5H3.75A1.25 1.25 0 0 0 2.5 3.75v5.5A1.25 1.25 0 0 0 3.75 10.5H5.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.25"
      />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <path
        d="M3.5 8.5 6.5 11.5 12.5 4.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function CopyCommand({ command }: { command: string }) {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");
  const statusId = useId();

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setCopyState("copied");
      window.setTimeout(() => setCopyState("idle"), 1600);
    } catch {
      setCopyState("error");
    }
  }

  const status =
    copyState === "copied"
      ? "Install command copied."
      : copyState === "error"
        ? "Copy failed. Select the command and copy it manually."
        : "";

  return (
    <div className={styles.copyBlock}>
      <div className={styles.copyCommand}>
        <code>{command}</code>
        <button
          type="button"
          onClick={() => void copy()}
          aria-describedby={statusId}
          aria-label={copyState === "copied" ? "Copied" : "Copy install command"}
        >
          {copyState === "copied" ? <CheckIcon /> : <CopyIcon />}
        </button>
      </div>
      <p id={statusId} className={styles.copyStatus} aria-live="polite">
        {status}
      </p>
    </div>
  );
}
