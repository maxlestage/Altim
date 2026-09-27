import { useEffect, useRef } from "react";

/** Grille synthwave animée + particules, dessinée sur un canvas plein écran. */
export function Background() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    let w = 0;
    let h = 0;
    let raf = 0;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const particles = Array.from({ length: 70 }, () => ({
      x: Math.random(),
      y: Math.random(),
      s: Math.random() * 1.6 + 0.3,
      v: Math.random() * 0.0004 + 0.0001,
    }));

    const resize = () => {
      w = window.innerWidth;
      h = window.innerHeight;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    const draw = (t: number) => {
      ctx.clearRect(0, 0, w, h);
      const horizon = h * 0.55;
      const offset = reduce ? 0 : (t / 2200) % 1;

      const grad = ctx.createLinearGradient(0, horizon, 0, h);
      grad.addColorStop(0, "rgba(0,240,255,0)");
      grad.addColorStop(0.4, "rgba(0,240,255,0.18)");
      grad.addColorStop(1, "rgba(255,43,214,0.28)");
      ctx.strokeStyle = grad;
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let i = 0; i < 22; i++) {
        const p = (i + offset) / 22;
        const y = horizon + (h - horizon) * p * p;
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
      }
      for (let i = -16; i <= 16; i++) {
        ctx.moveTo(w / 2 + i * 8, horizon);
        ctx.lineTo(w / 2 + (i * w) / 5, h);
      }
      ctx.stroke();

      for (const p of particles) {
        if (!reduce) p.y -= p.v;
        if (p.y < 0) p.y = 1;
        ctx.fillStyle = `rgba(160,240,255,${0.25 + p.s / 4})`;
        ctx.fillRect(p.x * w, p.y * h, p.s, p.s);
      }
      if (!reduce) raf = requestAnimationFrame(draw);
    };

    resize();
    window.addEventListener("resize", resize);
    raf = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", resize);
    };
  }, []);

  return (
    <div className="bg" aria-hidden>
      <div className="orb orb-a" />
      <div className="orb orb-b" />
      <canvas ref={ref} />
      <div className="scanlines" />
    </div>
  );
}
