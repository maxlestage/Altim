import { useEffect, useRef, useState } from "react";
import { fetchTicks, type Tick } from "./market";

/** Cours rafraîchis toutes les 15 s. */
export function useTicks(): Tick[] {
  const [ticks, setTicks] = useState<Tick[]>([]);
  useEffect(() => {
    let alive = true;
    const load = () =>
      fetchTicks()
        .then((t) => alive && setTicks(t))
        .catch(() => {});
    load();
    const id = setInterval(load, 15_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);
  return ticks;
}

/** Ajoute la classe `visible` quand l'élément entre à l'écran. */
export function useReveal<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const obs = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) {
          el.classList.add("visible");
          obs.disconnect();
        }
      },
      { threshold: 0.15 },
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, []);
  return ref;
}
