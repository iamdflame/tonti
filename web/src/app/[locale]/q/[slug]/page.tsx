import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { Nav } from '@/components/Nav';
import { Footer } from '@/components/landing/Sections';
import { format as f, getDictionary, hasLocale } from '@/i18n';
import { countries } from '@/sdk/countries.ts';
import { fromSlug, toQuery } from '@/lib/slug';
import { serverQuote } from '@/lib/server-quote';
import { num, pct, usd } from '@/lib/format';

export const revalidate = 3600;

export async function generateMetadata({ params }: PageProps<'/[locale]/q/[slug]'>): Promise<Metadata> {
  const { locale, slug } = await params;
  const a = fromSlug(slug);
  if (!hasLocale(locale) || !a) return {};
  const t = getDictionary(locale);
  const title = a.who === 'mother' ? t.hero.title : a.who === 'father' ? t.hero.titleFather : t.hero.titleMe;
  return { title: `${title} · Tonti`, description: t.meta.description };
}

export default async function SharedQuote({ params }: PageProps<'/[locale]/q/[slug]'>) {
  const { locale, slug } = await params;
  const a = fromSlug(slug);
  if (!hasLocale(locale) || !a) notFound();
  const t = getDictionary(locale);
  const q = await serverQuote(a).catch(() => null);
  const country = countries.find((c) => c.iso3 === a.country)?.name ?? a.country;
  const title = a.who === 'mother' ? t.hero.title : a.who === 'father' ? t.hero.titleFather : t.hero.titleMe;
  const solo = a.who === 'me' ? t.result.soloMe : a.who === 'father' ? t.result.soloHe : t.result.soloShe;
  return (
    <>
      <Nav />
      <main id="main" tabIndex={-1} className="outline-none dusk min-h-[80svh] bg-gradient-to-b from-bay via-bay to-night pt-28 pb-20 text-foreground">
        <div className="mx-auto max-w-3xl px-4 md:px-8">
          <h1 className="text-title font-extrabold text-white">{title}</h1>
          <p className="mt-4 text-body-l text-mist">
            {country} · {a.born} · {f(t.form.startAge + ' {age}', { age: a.startAge })} · {usd(locale, a.lump, 0)} + {usd(locale, a.monthly, 0)}
          </p>
          {q ? (
            <div className="mt-10 space-y-4 rounded-3xl bg-night/80 p-6 ring-1 ring-white/10 sm:p-8">
              <p className="font-mono text-[clamp(2.75rem,10vw,4.5rem)] leading-none font-bold text-lamp tabular">{usd(locale, q.incomeStart.p50)}</p>
              <p className="text-body-l font-bold text-white">
                {f(t.result.perMonth, { amount: '' }).trim()} · {t.result.forLife}
              </p>
              <p className="text-body text-foreground/90">{f(t.result.range, { low: usd(locale, q.incomeStart.p10), high: usd(locale, q.incomeStart.p90) })}</p>
              <p className="text-body text-white">{f(solo, { amount: usd(locale, q.incomeStart.p50), age: num(locale, q.soloRunoutAge, 0), chance: pct(locale, q.soloOutliveProbability) })}</p>
              <p className="text-caption text-mist">{t.result.computed}</p>
            </div>
          ) : (
            <p role="alert" className="mt-10 rounded-2xl bg-night/70 p-5 text-body">{t.result.error}</p>
          )}
          <Link href={`/${locale}${toQuery(a)}#ask`} className="press mt-8 inline-flex min-h-14 items-center justify-center rounded-xl bg-lamp px-6 text-body-l font-extrabold text-ink hover:bg-lamp-soft">
            {t.cta.title}
          </Link>
        </div>
      </main>
      <Footer t={t} />
    </>
  );
}
