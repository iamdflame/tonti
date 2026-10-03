import { loadFont as loadNext } from '@remotion/google-fonts/AtkinsonHyperlegibleNext';
import { loadFont as loadMono } from '@remotion/google-fonts/AtkinsonHyperlegibleMono';
import { Easing } from 'remotion';

// The site's own identity (web/DESIGN.md): Lifeline at dusk.
export const C = {
  night: '#0D1A30',
  bay: '#13233F',
  bay2: '#1C3157',
  haze: '#EEF2F6',
  paper: '#FFFFFF',
  ink: '#14213D',
  ink2: '#4B5873',
  mist: '#A9B4C6',
  lamp: '#FFB23F', // income figures and the one call to action only
  lampSoft: '#FFD48A',
  coral: '#F2785C', // the horizon only
  ok: '#1F7A4D',
} as const;

export const sans = loadNext('normal', { weights: ['400', '500', '700', '800'], subsets: ['latin'] }).fontFamily;
export const mono = loadMono('normal', { weights: ['400', '700'], subsets: ['latin'] }).fontFamily;

// DESIGN.md: ease-out cubic-bezier(0.23, 1, 0.32, 1), no bounce.
export const out = Easing.bezier(0.23, 1, 0.32, 1);
export const inOut = Easing.bezier(0.65, 0, 0.35, 1);

export const FPS = 30;
export const W = 1920;
export const H = 1080;
export const s = (seconds: number) => Math.round(seconds * FPS);
