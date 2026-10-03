import { useLayoutEffect, useRef, useState } from 'react';
import { continueRender, delayRender } from 'remotion';

// The site's dusk over Manila Bay (web/src/components/sky/Sky.tsx), the same fragment shader,
// drawn once per video frame. `sun` is 0 (set) to 1 (high); on the site it is the member's chance
// of being alive, so the light in the sky is a survival curve.
const FRAG = `#version 300 es
precision highp float;
uniform vec2 u_res;
uniform float u_sun;
uniform float u_wide;
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
  float horizon = 0.2;
  float sx = u_wide;
  vec2 sun = vec2(sx, horizon + mix(-0.04, 0.56, u_sun));
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

type GL = { ctx: WebGL2RenderingContext; res: WebGLUniformLocation | null; sun: WebGLUniformLocation | null; wide: WebGLUniformLocation | null };

export function Sky({ sun, sunX = 0.82, width = 1920, height = 1080, style }: { sun: number; sunX?: number; width?: number; height?: number; style?: React.CSSProperties }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const gl = useRef<GL | null>(null);
  const [handle] = useState(() => delayRender('dusk shader'));

  useLayoutEffect(() => {
    const c = canvas.current!;
    const ctx = c.getContext('webgl2', { antialias: false, alpha: false, preserveDrawingBuffer: true });
    if (!ctx) throw new Error('WebGL2 unavailable: render with --gl=swangle');
    const compile = (type: number, src: string) => {
      const sh = ctx.createShader(type)!;
      ctx.shaderSource(sh, src);
      ctx.compileShader(sh);
      if (!ctx.getShaderParameter(sh, ctx.COMPILE_STATUS)) throw new Error(ctx.getShaderInfoLog(sh) ?? 'shader');
      return sh;
    };
    const prog = ctx.createProgram()!;
    ctx.attachShader(prog, compile(ctx.VERTEX_SHADER, VERT));
    ctx.attachShader(prog, compile(ctx.FRAGMENT_SHADER, FRAG));
    ctx.linkProgram(prog);
    ctx.useProgram(prog);
    const buf = ctx.createBuffer();
    ctx.bindBuffer(ctx.ARRAY_BUFFER, buf);
    ctx.bufferData(ctx.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), ctx.STATIC_DRAW);
    const p = ctx.getAttribLocation(prog, 'p');
    ctx.enableVertexAttribArray(p);
    ctx.vertexAttribPointer(p, 2, ctx.FLOAT, false, 0, 0);
    gl.current = { ctx, res: ctx.getUniformLocation(prog, 'u_res'), sun: ctx.getUniformLocation(prog, 'u_sun'), wide: ctx.getUniformLocation(prog, 'u_wide') };
  }, []);

  // Drawn synchronously in the layout phase, so each frame's screenshot already holds its sky.
  const released = useRef(false);
  useLayoutEffect(() => {
    const g = gl.current;
    if (!g) return;
    g.ctx.viewport(0, 0, width, height);
    g.ctx.uniform2f(g.res, width, height);
    g.ctx.uniform1f(g.sun, sun);
    g.ctx.uniform1f(g.wide, sunX);
    g.ctx.drawArrays(g.ctx.TRIANGLES, 0, 3);
    g.ctx.finish();
    if (!released.current) {
      released.current = true;
      continueRender(handle);
    }
  }, [sun, sunX, width, height, handle]);

  return <canvas ref={canvas} width={width} height={height} style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', ...style }} />;
}
