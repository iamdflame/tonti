'use client';

import { useEffect, useRef, useState } from 'react';
import { useReducedMotion } from '@/lib/motion';

// The dusk over Manila Bay (web/DESIGN.md). One full-screen quad, drawn only when the sun moves:
// no animation loop, no continuous GPU cost. `sun` is 0 (set) to 1 (high); the page sets it to
// the chance of being alive, read from the chain, so the light in the sky is her survival curve.
const FRAG = `#version 300 es
precision highp float;
uniform vec2 u_res;
uniform float u_sun;
out vec4 o;
const vec3 NIGHT = vec3(0.051, 0.102, 0.188);
const vec3 BAY   = vec3(0.075, 0.137, 0.247);
const vec3 BAY2  = vec3(0.110, 0.192, 0.341);
const vec3 CORAL = vec3(0.949, 0.471, 0.361);
const vec3 LAMP  = vec3(1.000, 0.698, 0.247);
const vec3 LAMPS = vec3(1.000, 0.831, 0.541);
float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
void main() {
  vec2 uv = gl_FragCoord.xy / u_res;
  float aspect = u_res.x / u_res.y;
  bool wide = aspect > 1.0;
  float horizon = wide ? 0.2 : 0.12;
  float sx = wide ? 0.9 : 0.84;
  vec2 sun = vec2(sx, horizon + mix(-0.04, wide ? 0.56 : 0.34, u_sun));
  vec2 d = vec2((uv.x - sun.x) * aspect, uv.y - sun.y);
  float dist = length(d);
  float dusk = 1.0 - u_sun;
  float h = clamp((uv.y - horizon) / (1.0 - horizon), 0.0, 1.0);
  vec3 col = mix(BAY2, NIGHT, smoothstep(0.1, 1.0, h));
  float band = (1.0 - smoothstep(0.0, 0.6, h)) * (1.0 - h);
  col = mix(col, mix(CORAL, LAMPS, 0.3), band * (0.2 + 0.6 * dusk));
  col += LAMP * exp(-dist * 5.0) * (0.45 + 0.4 * dusk);
  float disc = smoothstep(0.052, 0.046, dist) * step(horizon, uv.y);
  col = mix(col, LAMPS, disc);
  if (uv.y < horizon) {
    float depth = (horizon - uv.y) / horizon;
    vec3 sea = mix(BAY, NIGHT, depth * 0.9);
    float up = smoothstep(-0.05, 0.2, sun.y - horizon);
    float streak = exp(-abs((uv.x - sun.x) * aspect) * (14.0 + 30.0 * depth)) * (1.0 - depth);
    float ripple = 0.55 + 0.45 * sin(uv.y * 700.0 + sin(uv.x * 37.0) * 2.5);
    sea += LAMP * streak * ripple * 0.42 * (0.25 + 0.75 * up);
    sea = mix(sea, CORAL * 0.55, exp(-(horizon - uv.y) * 70.0) * 0.4 * (0.3 + dusk));
    col = sea;
  }
  col += (hash(gl_FragCoord.xy) - 0.5) * (2.0 / 255.0);
  o = vec4(col, 1.0);
}`;
const VERT = `#version 300 es
in vec2 p;
void main() { gl_Position = vec4(p, 0.0, 1.0); }`;

export function Sky({ sun, className }: { sun: number; className?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const gl = useRef<{ ctx: WebGL2RenderingContext; loc: { res: WebGLUniformLocation | null; sun: WebGLUniformLocation | null } } | null>(null);
  const shown = useRef(sun);
  const [webgl, setWebgl] = useState(true);
  // The shader has drawn its first frame; until then the CSS dusk shows.
  const [lit, setLit] = useState(false);
  const reduced = useReducedMotion();

  const draw = (s: number) => {
    const g = gl.current;
    const c = canvas.current;
    if (!g || !c) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
    const w = Math.round(c.clientWidth * dpr);
    const h = Math.round(c.clientHeight * dpr);
    if (c.width !== w || c.height !== h) {
      c.width = w;
      c.height = h;
    }
    g.ctx.viewport(0, 0, w, h);
    g.ctx.uniform2f(g.loc.res, w, h);
    g.ctx.uniform1f(g.loc.sun, s);
    g.ctx.drawArrays(g.ctx.TRIANGLES, 0, 3);
  };

  useEffect(() => {
    // On phones the GPU context and shader compile wait for the first touch, scroll or key: the CSS
    // dusk underneath is the first paint, and the page is interactive sooner on a budget Android.
    const lazy = window.matchMedia('(pointer: coarse)').matches;
    let stop: (() => void) | undefined;
    const events = ['pointerdown', 'touchstart', 'scroll', 'keydown'] as const;
    const off = () => events.forEach((e) => window.removeEventListener(e, go));
    function go() {
      off();
      stop = start();
    }
    if (!lazy) stop = start();
    else events.forEach((e) => window.addEventListener(e, go, { passive: true }));
    return () => {
      off();
      stop?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function start(): (() => void) | undefined {
    const c = canvas.current;
    const weak = typeof navigator !== 'undefined' && (navigator.hardwareConcurrency ?? 4) <= 2;
    const ctx = !weak && c ? c.getContext('webgl2', { antialias: false, alpha: false, powerPreference: 'low-power' }) : null;
    if (!ctx) {
      setWebgl(false);
      return undefined;
    }
    const compile = (type: number, src: string) => {
      const s = ctx.createShader(type)!;
      ctx.shaderSource(s, src);
      ctx.compileShader(s);
      return s;
    };
    const prog = ctx.createProgram()!;
    ctx.attachShader(prog, compile(ctx.VERTEX_SHADER, VERT));
    ctx.attachShader(prog, compile(ctx.FRAGMENT_SHADER, FRAG));
    ctx.linkProgram(prog);
    if (!ctx.getProgramParameter(prog, ctx.LINK_STATUS)) {
      setWebgl(false);
      return undefined;
    }
    ctx.useProgram(prog);
    const buf = ctx.createBuffer();
    ctx.bindBuffer(ctx.ARRAY_BUFFER, buf);
    ctx.bufferData(ctx.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), ctx.STATIC_DRAW);
    const p = ctx.getAttribLocation(prog, 'p');
    ctx.enableVertexAttribArray(p);
    ctx.vertexAttribPointer(p, 2, ctx.FLOAT, false, 0, 0);
    gl.current = { ctx, loc: { res: ctx.getUniformLocation(prog, 'u_res'), sun: ctx.getUniformLocation(prog, 'u_sun') } };
    draw(shown.current);
    setLit(true);
    const ro = new ResizeObserver(() => draw(shown.current));
    ro.observe(c!);
    return () => ro.disconnect();
  }

  // The one orchestrated moment: the sun settles on her curve (900 ms, ease-out, interruptible).
  useEffect(() => {
    if (!gl.current) {
      shown.current = sun;
      return;
    }
    const from = shown.current;
    if (reduced || Math.abs(from - sun) < 1e-3) {
      shown.current = sun;
      draw(sun);
      return;
    }
    let raf = 0;
    const t0 = performance.now();
    const ease = (t: number) => 1 - Math.pow(1 - t, 4);
    const step = (now: number) => {
      const t = Math.min(1, (now - t0) / 900);
      shown.current = from + (sun - from) * ease(t);
      draw(shown.current);
      if (t < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sun, reduced]);

  return (
    // The caller positions it (absolutely, behind the hero); that also anchors the layers below.
    <div aria-hidden="true" className={className ?? 'absolute inset-0'}>
      <div
        className="absolute inset-0"
        style={{
          background: `radial-gradient(55% 42% at 86% ${80 - sun * 40}%, rgba(255,178,63,0.55), transparent 70%), linear-gradient(to top, #0d1a30 0%, #13233f 15%, #f2785c 17%, #1c3157 42%, #0d1a30 100%)`,
        }}
      />
      {webgl && <canvas ref={canvas} className={`absolute inset-0 block h-full w-full transition-opacity duration-500 ${lit ? 'opacity-100' : 'opacity-0'}`} />}
    </div>
  );
}
