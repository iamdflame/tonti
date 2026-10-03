// Recording the live site for the demo video: real Chrome, the real site, mainnet.
//
// The build machine can't screencast 1080p in real time (≈10 fps), so time is virtual: the page's
// clock (timers, requestAnimationFrame, performance.now, Date) is Playwright's fake clock, CSS
// animations and transitions are seeked to the same virtual time, and exactly one frame is captured
// every 1/30 s of virtual time, however long the machine takes to draw it. The network is never
// faked: while a request to the chain is in flight, `real()` lets the clock run in real time and
// fills the video with what the screen showed, so a 1.2 s answer takes 1.2 s on screen and the
// site's own "computed in … s" reads the true figure.
import { chromium } from '@playwright/test';
import { mkdirSync, rmSync, writeFileSync, existsSync, copyFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

export const SITE = process.env.SITE ?? 'https://tonti-life.vercel.app';
export const TAKES = existsSync('/media/dflame/UNIQ/arbit/video/takes') ? '/media/dflame/UNIQ/arbit/video/takes' : '/dev/shm/tonti-video/takes';
// Frames go to the data drive when it is mounted: in RAM they competed with Chrome and the editor,
// and the machine went down mid-take.
const FRAMES = existsSync('/media/dflame/UNIQ/arbit/video') ? '/media/dflame/UNIQ/arbit/video/frames' : '/dev/shm/tonti-video/frames';
const FPS = 30;
const STEP = 1000 / FPS;
export const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));

export async function browser() {
  return chromium.launch({
    executablePath: '/opt/google/chrome/chrome',
    // WebGL (the dusk sky) on the machine's Intel GPU through Vulkan; SwiftShader is 10x slower.
    args: ['--use-angle=vulkan', '--enable-features=Vulkan', '--ignore-gpu-blocklist', '--hide-scrollbars', '--force-color-profile=srgb'],
  });
}

// A visible pointer and touch ripples: headless Chrome draws neither. Animated with
// requestAnimationFrame, so they live on the same virtual clock as the page.
const OVERLAY = ({ touch }) => {
  const install = () => {
    if (document.getElementById('__rec')) return;
    const root = document.createElement('div');
    root.id = '__rec';
    root.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483647';
    const ptr = document.createElement('div');
    ptr.style.cssText = touch
      ? 'position:absolute;left:0;top:0;width:46px;height:46px;margin:-23px 0 0 -23px;border-radius:50%;background:rgba(255,255,255,.28);border:2px solid rgba(255,255,255,.9);box-shadow:0 2px 12px rgba(0,0,0,.28);opacity:0'
      : 'position:absolute;left:0;top:0;width:28px;height:28px;opacity:0;filter:drop-shadow(0 2px 3px rgba(0,0,0,.35))';
    if (!touch) ptr.innerHTML = '<svg viewBox="0 0 24 24" width="28" height="28"><path d="M4 2.5 L4 19.5 L8.6 15.3 L11.4 21.6 L14.3 20.3 L11.5 14.1 L17.8 14.1 Z" fill="#fff" stroke="#14213D" stroke-width="1.4" stroke-linejoin="round"/></svg>';
    root.appendChild(ptr);
    document.documentElement.appendChild(root);
    let x = -100, y = -100, op = 0, opTarget = 0;
    const place = () => { ptr.style.transform = `translate(${x}px, ${y}px)`; ptr.style.opacity = String(op); };
    const ease = (t) => 1 - Math.pow(1 - t, 3);
    const tick = () => { op += (opTarget - op) * 0.35; place(); requestAnimationFrame(tick); };
    requestAnimationFrame(tick);
    window.__rec = {
      show(v = true) { opTarget = v ? 1 : 0; },
      jump(nx, ny) { x = nx; y = ny; place(); },
      move(nx, ny, ms) {
        const x0 = x, y0 = y, t0 = performance.now(), len = Math.hypot(nx - x0, ny - y0) || 1;
        const bend = (Math.random() - 0.5) * 0.16 * len; // a hand's gentle arc, not a ruler
        const step = (now) => {
          const t = Math.min(1, (now - t0) / ms), e = ease(t), b = Math.sin(Math.PI * e) * bend;
          x = x0 + (nx - x0) * e + b * ((ny - y0) / len);
          y = y0 + (ny - y0) * e - b * ((nx - x0) / len);
          if (t < 1) requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      },
      ripple(px = x, py = y) {
        const r = document.createElement('div');
        r.style.cssText = `position:absolute;left:${px}px;top:${py}px;width:18px;height:18px;margin:-9px 0 0 -9px;border-radius:50%;background:rgba(255,178,63,.5)`;
        root.appendChild(r);
        const t0 = performance.now();
        const step = (now) => {
          const t = Math.min(1, (now - t0) / 520), e = 1 - Math.pow(1 - t, 3);
          r.style.transform = `scale(${1 + 3.4 * e})`;
          r.style.opacity = String(1 - e);
          t < 1 ? requestAnimationFrame(step) : r.remove();
        };
        requestAnimationFrame(step);
      },
      scroll(dy, ms) {
        const y0 = window.scrollY, t0 = performance.now();
        const e = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
        const step = (now) => { const t = Math.min(1, (now - t0) / ms); window.scrollTo(0, y0 + dy * e(t)); if (t < 1) requestAnimationFrame(step); };
        requestAnimationFrame(step);
      },
    };
    // CSS animations and transitions follow the virtual clock: each frame, every animation is
    // paused and set to the time elapsed since it first appeared.
    const seen = new Map();
    window.__seek = () => {
      const now = performance.now();
      for (const a of document.getAnimations()) {
        if (!seen.has(a)) seen.set(a, now - (a.currentTime ?? 0) * 0);
        if (a.playState !== 'paused') a.pause();
        a.currentTime = now - seen.get(a);
      }
    };
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install);
  else install();
};

export async function context(b, { phone = false, dpr = phone ? 3 : 1.5, width = phone ? 430 : 1920, height = phone ? 932 : 1080, ua } = {}) {
  const ctx = await b.newContext({
    viewport: { width, height },
    deviceScaleFactor: dpr,
    isMobile: phone,
    hasTouch: phone,
    userAgent: ua ?? (phone ? 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1' : undefined),
    colorScheme: 'light',
    reducedMotion: 'no-preference',
  });
  await ctx.addInitScript(OVERLAY, { touch: phone });
  return ctx;
}

/** A virtual-time recording session for one page. */
export async function session(page, name) {
  const dir = join(FRAMES, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const cdp = await page.context().newCDPSession(page);
  let n = 0;
  const marks = [];
  const vp = page.viewportSize();
  const dpr = await page.evaluate(() => window.devicePixelRatio);
  // At full device resolution (a plain capture is CSS-sized), clipped to the visible viewport.
  const shoot = async () => {
    const { x, y } = await page.evaluate(() => ({ x: window.scrollX, y: window.scrollY }));
    const { data } = await cdp.send('Page.captureScreenshot', { format: 'jpeg', quality: 93, optimizeForSpeed: true, clip: { x, y, width: vp.width, height: vp.height, scale: dpr } });
    const file = join(dir, `${String(n++).padStart(6, '0')}.jpg`);
    writeFileSync(file, Buffer.from(data, 'base64'));
    return file;
  };
  const vt = {
    /** Advances virtual time by `ms`, one captured frame per 1/30 s. */
    async wait(ms) {
      for (let k = 0, steps = Math.max(1, Math.round(ms / STEP)); k < steps; k++) {
        await page.clock.runFor(STEP);
        await page.evaluate(() => window.__seek?.());
        await shoot();
      }
    },
    /** Runs `fn` (a network round trip) in real time. A fast ticker keeps page time on the wall clock
     * (every ~15 ms), so a 0.3 s call measures 0.3 s on the page; a slower loop captures what the screen
     * shows and fills the video to the wait's true length. */
    async real(fn) {
      const t0 = Date.now();
      let last = t0, filled = 0, done = false;
      const job = fn();
      job.then(() => (done = true), () => (done = true));
      const ticker = (async () => {
        while (!done) {
          const now = Date.now();
          if (now > last) await page.clock.runFor(now - last);
          last = now;
          await sleep(12);
        }
      })();
      while (!done) {
        await page.evaluate(() => window.__seek?.());
        const file = await shoot();
        const due = Math.floor(((Date.now() - t0) / 1000) * FPS);
        for (; filled < due; filled++) copyFileSync(file, join(dir, `${String(n++).padStart(6, '0')}.jpg`));
        if (Date.now() - t0 > 60_000) break;
        await sleep(150);
      }
      await job;
      await ticker;
      if (Date.now() > last) await page.clock.runFor(Date.now() - last);
      return { seconds: (Date.now() - t0) / 1000 };
    },
    mark(label) { marks.push({ label, frame: n, at: +(n / FPS).toFixed(2) }); },
    frames: () => n,
  };
  const finish = async () => {
    const out = join(TAKES, `${name}.mp4`);
    execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-framerate', String(FPS), '-i', join(dir, '%06d.jpg'),
      '-vf', 'scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p', '-c:v', 'libx264', '-preset', 'slow', '-crf', '12',
      '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-movflags', '+faststart', out]);
    const stats = { name, out, frames: n, seconds: +(n / FPS).toFixed(2), marks };
    writeFileSync(join(TAKES, `${name}.json`), JSON.stringify(stats, null, 1));
    rmSync(dir, { recursive: true, force: true });
    return stats;
  };
  return { vt, finish };
}

/** Human-paced actions with the visible pointer, on the virtual clock. */
export function hands(page, vt, { phone = false } = {}) {
  const center = async (loc) => {
    const b = await loc.boundingBox();
    return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
  };
  return {
    async start(x, y) { await page.evaluate(([x, y]) => { window.__rec.jump(x, y); window.__rec.show(true); }, [x, y]); await vt.wait(300); },
    async hide(ms = 300) { await page.evaluate(() => window.__rec.show(false)); await vt.wait(ms); },
    async moveTo(loc, ms = 700) {
      const p = await center(loc);
      await page.evaluate(([x, y, ms]) => window.__rec.move(x, y, ms), [p.x, p.y, ms]);
      await page.mouse.move(p.x, p.y);
      await vt.wait(ms);
      return p;
    },
    async tap(loc, { ms = 650, hold = 100 } = {}) {
      const p = phone ? await center(loc) : await this.moveTo(loc, ms);
      if (phone) await page.evaluate(([x, y]) => { window.__rec.jump(x, y); window.__rec.show(true); }, [p.x, p.y]);
      await page.evaluate(([x, y]) => window.__rec.ripple(x, y), [p.x, p.y]);
      await page.mouse.move(p.x, p.y);
      await page.mouse.down();
      await vt.wait(hold);
      await page.mouse.up();
      await vt.wait(120);
      if (phone) await page.evaluate(() => window.__rec.show(false));
      return p;
    },
    async type(loc, text, { base = 95 } = {}) {
      await loc.focus();
      await loc.selectText().catch(() => {});
      await page.keyboard.press('Backspace');
      for (const ch of text) {
        await page.keyboard.type(ch);
        await vt.wait(base + Math.random() * base * 0.7);
      }
    },
    async scroll(dy, ms = 1400) {
      await page.evaluate(([dy, ms]) => window.__rec.scroll(dy, ms), [dy, ms]);
      await vt.wait(ms + 60);
    },
    async scrollTo(loc, { offset = 120, ms = 1400 } = {}) {
      const top = await loc.evaluate((el) => el.getBoundingClientRect().top);
      await this.scroll(top - offset, ms);
    },
  };
}

/** Opens `url` with the virtual clock installed and lets the page settle. */
export async function open(page, url, { settle = 2500 } = {}) {
  await page.clock.install();
  await page.goto(url, { waitUntil: 'load' });
  await page.clock.resume();
  await page.waitForLoadState('networkidle').catch(() => {});
  await sleep(settle);
  await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1500);
}
