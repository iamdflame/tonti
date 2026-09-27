import { ImageResponse } from 'next/og';
import { countries } from '@/sdk/countries.ts';
import { format as f, getDictionary, hasLocale } from '@/i18n';
import { fromSlug } from '@/lib/slug';
import { serverQuote } from '@/lib/server-quote';
import { usd } from '@/lib/format';

export const size = { width: 1200, height: 630 };
export const contentType = 'image/png';
export const revalidate = 86400;
export const alt = 'A lifelong monthly income, computed on Robinhood Chain';

async function font(family: string, weight: number) {
  const css = await (await fetch(`https://fonts.googleapis.com/css2?family=${family}:wght@${weight}`)).text();
  const url = css.match(/url\((https:[^)]+\.ttf)\)/)?.[1];
  return url ? (await fetch(url)).arrayBuffer() : null;
}

/** The picture a shared quote shows in Messenger and WhatsApp: her income, computed on the chain. */
export default async function Image({ params }: { params: Promise<{ locale: string; slug: string }> }) {
  const { locale, slug } = await params;
  const a = fromSlug(slug);
  const loc = hasLocale(locale) ? locale : 'en';
  const t = getDictionary(loc);
  const [sans, mono, q] = await Promise.all([font('Atkinson+Hyperlegible+Next', 800), font('Atkinson+Hyperlegible+Mono', 700), a ? serverQuote(a).catch(() => null) : null]);
  const title = !a ? t.meta.title : a.who === 'mother' ? t.hero.title : a.who === 'father' ? t.hero.titleFather : t.hero.titleMe;
  const country = a ? (countries.find((c) => c.iso3 === a.country)?.name ?? a.country) : '';
  return new ImageResponse(
    (
      <div style={{ width: '100%', height: '100%', display: 'flex', flexDirection: 'column', justifyContent: 'space-between', padding: 64, color: '#f3f5f8', background: 'linear-gradient(180deg,#0d1a30 0%,#1c3157 52%,#f2785c 78%,#13233f 79%,#0d1a30 100%)', fontFamily: 'Atkinson' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div style={{ display: 'flex', fontSize: 40, fontWeight: 800, letterSpacing: -1 }}>tonti</div>
          <div style={{ display: 'flex', width: 96, height: 96, borderRadius: 48, background: '#ffd48a', boxShadow: '0 0 80px 30px rgba(255,178,63,0.55)' }} />
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <div style={{ display: 'flex', fontSize: 44, fontWeight: 800, lineHeight: 1.1, maxWidth: 980 }}>{title}</div>
          {q && (
            <div style={{ display: 'flex', alignItems: 'baseline', gap: 20 }}>
              <div style={{ display: 'flex', fontFamily: 'Mono', fontSize: 120, fontWeight: 700, color: '#ffb23f', letterSpacing: -4 }}>{usd(loc, q.incomeStart.p50)}</div>
              <div style={{ display: 'flex', fontSize: 40, fontWeight: 800 }}>{`${f(t.result.perMonth, { amount: '' }).trim()}, ${t.result.forLife}`}</div>
            </div>
          )}
          <div style={{ display: 'flex', fontSize: 26, color: '#dfe5ee' }}>{`${a ? `${country} · ${a.born} · ${a.startAge}+ · ` : ''}${t.result.computed}`}</div>
        </div>
      </div>
    ),
    {
      ...size,
      fonts: [
        ...(sans ? [{ name: 'Atkinson', data: sans, weight: 800 as const, style: 'normal' as const }] : []),
        ...(mono ? [{ name: 'Mono', data: mono, weight: 700 as const, style: 'normal' as const }] : []),
      ],
    },
  );
}
