/**
 * Simulated portfolio (paper trading), kept in this browser only (localStorage "altim.paper.v1"); nothing is sent
 * to the server. A saved state that fails isPaperState is ignored with a message, never a crash; the unreadable
 * text is kept aside ("altim.paper.v1.invalid") the first time a new simulation overwrites it.
 */
import { useSyncExternalStore } from "react";
import { DEFAULT_CAPITAL, newPaper, type PaperState } from "../engine/paper";
import { PAPER_KEY, parseSavedPaper, type SavedPaper } from "./paper-ui";

function read(): SavedPaper {
  try {
    return parseSavedPaper(localStorage.getItem(PAPER_KEY));
  } catch {
    return { state: null, error: null };
  }
}

let current: SavedPaper = typeof window === "undefined" ? { state: null, error: null } : read();
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

if (typeof window !== "undefined") {
  // Another tab changed the simulation: follow it.
  window.addEventListener("storage", (e) => {
    if (e.key !== PAPER_KEY) return;
    current = read();
    emit();
  });
}

export function usePaper(): SavedPaper {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => current,
    () => current,
  );
}

export const getPaper = () => current.state;

/** Saves a new state (the result of an engine function applied to getPaper()). */
export function setPaper(state: PaperState) {
  try {
    if (current.error) {
      const bad = localStorage.getItem(PAPER_KEY);
      if (bad != null) localStorage.setItem(`${PAPER_KEY}.invalid`, bad);
    }
    localStorage.setItem(PAPER_KEY, JSON.stringify(state));
  } catch {}
  current = { state, error: null };
  emit();
}

/** Starts (or restarts) the simulation: every position and trade is erased. */
export function resetPaper(capital: number = DEFAULT_CAPITAL, now = Date.now()) {
  setPaper(newPaper(capital, now));
}
