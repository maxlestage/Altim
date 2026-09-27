import { useSyncExternalStore, type MouseEvent } from "react";

const EVENT = "altim:navigate";

export function navigate(to: string, replace = false) {
  if (to === location.pathname + location.search) return;
  history[replace ? "replaceState" : "pushState"]({}, "", to);
  window.dispatchEvent(new Event(EVENT));
  window.scrollTo(0, 0);
}

export function usePath(): string {
  return useSyncExternalStore(
    (l) => {
      window.addEventListener("popstate", l);
      window.addEventListener(EVENT, l);
      return () => {
        window.removeEventListener("popstate", l);
        window.removeEventListener(EVENT, l);
      };
    },
    () => location.pathname,
    () => "/app",
  );
}

/** Internal link: in-app navigation without reloading the page. */
export function onLink(e: MouseEvent<HTMLAnchorElement>) {
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
  e.preventDefault();
  navigate(e.currentTarget.getAttribute("href")!);
}
