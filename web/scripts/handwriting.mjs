#!/usr/bin/env node
// Pre-renders the one handwritten phrase under the income ("for life" / "habambuhay") with Tegaki
// (Tillana, which also covers Devanagari for Hindi later) to self-drawing SVGs in public/hand/, so
// the page ships a few KB of SVG instead of a handwriting engine and its font. Run when the copy
// changes: node scripts/handwriting.mjs
import { execFileSync } from 'node:child_process';
const phrases = { 'for-life': 'for life', habambuhay: 'habambuhay' };
for (const [file, text] of Object.entries(phrases)) {
  for (const mode of ['once', 'static']) {
    execFileSync('npx', ['tegaki', text, '--font', 'tillana', '--mode', mode, '--size', '64', '--color', '#ffd48a', '-o', `public/hand/${file}-${mode}.svg`], { stdio: 'inherit' });
  }
}
