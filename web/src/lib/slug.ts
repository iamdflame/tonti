import type { Iso3, Sex } from '@/sdk/units.ts';
import { countries } from '@/sdk/countries.ts';

export type Who = 'mother' | 'father' | 'me';
export type Ask = { who: Who; sex: Sex; country: Iso3; born: number; startAge: number; lump: number; monthly: number };

const WHO = { mother: 'm', father: 'f', me: 'x' } as const;

/** A quote as a short, readable path: m-PHL-1966-62-3000-30 (me: xf/xm for the sex). */
export function toSlug(a: Ask): string {
  const w = a.who === 'me' ? `x${a.sex === 'female' ? 'f' : 'm'}` : WHO[a.who];
  return [w, a.country, a.born, a.startAge, Math.round(a.lump), Math.round(a.monthly)].join('-');
}

export function fromSlug(slug: string): Ask | null {
  const p = slug.split('-');
  if (p.length !== 6) return null;
  const [w, country, born, start, lump, monthly] = p;
  const who: Who | null = w === 'm' ? 'mother' : w === 'f' ? 'father' : w === 'xf' || w === 'xm' ? 'me' : null;
  if (!who || !countries.some((c) => c.iso3 === country)) return null;
  const sex: Sex = w === 'm' || w === 'xf' ? 'female' : 'male';
  const n = [born, start, lump, monthly].map(Number);
  if (n.some((x) => !Number.isFinite(x) || x < 0)) return null;
  const year = new Date().getUTCFullYear();
  if (n[0] < year - 80 || n[0] > year - 18 || n[1] < 50 || n[1] > 80 || n[2] > 10_000_000 || n[3] > 100_000) return null;
  return { who, sex, country: country as Iso3, born: n[0], startAge: n[1], lump: n[2], monthly: n[3] };
}

/** The same question, as the landing page's query string. */
export const toQuery = (a: Ask) => `?w=${a.who}&s=${a.sex[0]}&c=${a.country}&b=${a.born}&a=${a.startAge}&l=${a.lump}&m=${a.monthly}`;
