import { useEffect, useRef } from "react";
import { useColorMode } from "@docusaurus/theme-common";

/**
 * A lightweight, theme-aware particle-mesh canvas for the hero. Nodes drift and
 * link up when close, reading as a "network / engine" without any dependency.
 * SSR-safe (runs only in an effect), pauses when offscreen, and respects
 * prefers-reduced-motion (renders a single static frame instead of animating).
 */
export default function HeroCanvas({ className }) {
  const ref = useRef(null);
  const { colorMode } = useColorMode();

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return undefined;
    const ctx = canvas.getContext("2d");
    if (!ctx) return undefined;

    const reduce =
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const dark = colorMode === "dark";
    const rgb = dark ? "228, 228, 231" : "24, 24, 27";
    const dotA = dark ? 0.55 : 0.42;
    const lineA = dark ? 0.22 : 0.14;
    const LINK = 132;

    let width = 0;
    let height = 0;
    let nodes = [];
    let raf = 0;
    let running = true;

    const seed = () => {
      const rect = canvas.getBoundingClientRect();
      width = Math.max(1, rect.width);
      height = Math.max(1, rect.height);
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const target = Math.round((width * height) / 16000);
      const count = Math.min(84, Math.max(26, target));
      nodes = Array.from({ length: count }, () => ({
        x: Math.random() * width,
        y: Math.random() * height,
        vx: (Math.random() - 0.5) * 0.28,
        vy: (Math.random() - 0.5) * 0.28,
        r: Math.random() * 1.4 + 0.7,
      }));
    };

    const draw = () => {
      ctx.clearRect(0, 0, width, height);
      for (let i = 0; i < nodes.length; i += 1) {
        const a = nodes[i];
        for (let j = i + 1; j < nodes.length; j += 1) {
          const b = nodes[j];
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const dist = Math.hypot(dx, dy);
          if (dist < LINK) {
            const o = (1 - dist / LINK) * lineA;
            ctx.strokeStyle = `rgba(${rgb}, ${o})`;
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(a.x, a.y);
            ctx.lineTo(b.x, b.y);
            ctx.stroke();
          }
        }
      }
      for (const n of nodes) {
        ctx.fillStyle = `rgba(${rgb}, ${dotA})`;
        ctx.beginPath();
        ctx.arc(n.x, n.y, n.r, 0, Math.PI * 2);
        ctx.fill();
      }
    };

    const step = () => {
      if (!running) return;
      for (const n of nodes) {
        n.x += n.vx;
        n.y += n.vy;
        if (n.x < -10) n.x = width + 10;
        else if (n.x > width + 10) n.x = -10;
        if (n.y < -10) n.y = height + 10;
        else if (n.y > height + 10) n.y = -10;
      }
      draw();
      raf = requestAnimationFrame(step);
    };

    seed();
    if (reduce) {
      draw();
    } else {
      raf = requestAnimationFrame(step);
    }

    const onResize = () => {
      seed();
      if (reduce) draw();
    };
    window.addEventListener("resize", onResize);

    let io;
    if (typeof IntersectionObserver !== "undefined" && !reduce) {
      io = new IntersectionObserver(
        (entries) => {
          const visible = entries[0]?.isIntersecting ?? true;
          if (visible && !running) {
            running = true;
            raf = requestAnimationFrame(step);
          } else if (!visible && running) {
            running = false;
            cancelAnimationFrame(raf);
          }
        },
        { threshold: 0 },
      );
      io.observe(canvas);
    }

    return () => {
      running = false;
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", onResize);
      if (io) io.disconnect();
    };
  }, [colorMode]);

  return <canvas ref={ref} className={className} aria-hidden="true" />;
}
