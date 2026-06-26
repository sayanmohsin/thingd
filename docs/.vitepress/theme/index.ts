import DefaultTheme from "vitepress/theme";
import { h, onMounted } from "vue";
import "./custom.css";

function ParticleBackground() {
  let canvasRef: HTMLCanvasElement | null = null;

  onMounted(() => {
    if (!canvasRef) {
      return;
    }
    const ctx = canvasRef.getContext("2d");
    if (!ctx) {
      return;
    }

    canvasRef.width = window.innerWidth;
    canvasRef.height = window.innerHeight;

    const particles: { x: number; y: number; vx: number; vy: number; r: number; a: number }[] = [];
    const count = window.innerWidth < 480 ? 20 : window.innerWidth < 768 ? 35 : 60;
    for (let i = 0; i < count; i++) {
      particles.push({
        x: Math.random() * canvasRef.width,
        y: Math.random() * canvasRef.height,
        vx: (Math.random() - 0.5) * 0.4,
        vy: (Math.random() - 0.5) * 0.4,
        r: Math.random() * 1.5 + 0.5,
        a: Math.random() * 0.4 + 0.1,
      });
    }

    function resize() {
      if (!canvasRef) {
        return;
      }
      canvasRef.width = window.innerWidth;
      canvasRef.height = window.innerHeight;
    }
    window.addEventListener("resize", resize);

    function animate() {
      if (!canvasRef || !ctx) {
        return;
      }
      ctx.clearRect(0, 0, canvasRef.width, canvasRef.height);
      for (const p of particles) {
        p.x += p.vx;
        p.y += p.vy;
        if (p.x < 0 || p.x > canvasRef.width) {
          p.vx *= -1;
        }
        if (p.y < 0 || p.y > canvasRef.height) {
          p.vy *= -1;
        }
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(255,106,0,${p.a})`;
        ctx.fill();
      }
      for (let i = 0; i < particles.length; i++) {
        for (let j = i + 1; j < particles.length; j++) {
          const a = particles[i];
          const b = particles[j];
          if (!a || !b) {
            continue;
          }
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const dist = Math.sqrt(dx * dx + dy * dy);
          if (dist < 120) {
            ctx.beginPath();
            ctx.moveTo(a.x, a.y);
            ctx.lineTo(b.x, b.y);
            ctx.strokeStyle = `rgba(255,106,0,${0.06 * (1 - dist / 120)})`;
            ctx.stroke();
          }
        }
      }
      requestAnimationFrame(animate);
    }
    animate();
  });

  return h("canvas", {
    ref: (el: HTMLCanvasElement | null) => {
      canvasRef = el;
    },
    style: "position:fixed;inset:0;z-index:0;pointer-events:none;",
  });
}

export default {
  extends: DefaultTheme,
  Layout() {
    return h(DefaultTheme.Layout, null, {
      "layout-top": () => h(ParticleBackground),
    });
  },
};
