'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useI18n } from '@/i18n/client';
import { locales, localeName } from '@/i18n';

/** The wordmark: "tonti", its tittle a lamp that stays lit. */
export function Wordmark({ className }: { className?: string }) {
  return (
    <span className={`relative inline-flex items-baseline text-[1.6rem] leading-none font-extrabold tracking-[-0.03em] ${className ?? ''}`}>
      <span aria-hidden="true" className="inline-flex items-baseline">
        tont
        <span className="relative">
          ı
          <span className="absolute -top-[0.28em] left-1/2 size-[0.3em] -translate-x-1/2 rounded-full bg-lamp shadow-[0_0_12px_2px_rgba(255,178,63,0.7)]" />
        </span>
      </span>
      <span className="sr-only">Tonti</span>
    </span>
  );
}

export function Nav({ tone = 'dusk' }: { tone?: 'dusk' | 'day' }) {
  const { t, locale } = useI18n();
  const path = usePathname();
  const rest = path.replace(/^\/(en|fil)(?=\/|$)/, '');
  const dark = tone === 'dusk';
  return (
    <header className={`${dark ? 'dusk absolute text-white' : 'sticky bg-haze/90 text-ink backdrop-blur-md'} inset-x-0 top-0 z-40`}>
      <a href="#main" className="sr-only focus:not-sr-only focus:absolute focus:top-3 focus:left-3 focus:z-50 focus:rounded-lg focus:bg-lamp focus:px-4 focus:py-3 focus:text-ink">
        {t.nav.skip}
      </a>
      <nav className="mx-auto flex h-18 max-w-6xl items-center justify-between gap-4 px-4 md:px-8" aria-label="Tonti">
        <Link href={`/${locale}`} className="press rounded-md px-1 py-2">
          <Wordmark />
        </Link>
        <div className="flex items-center gap-1 sm:gap-2">
          <ul className="hidden items-center gap-1 md:flex">
            <li><a href={`/${locale}#how`} className={`rounded-lg px-3 py-2.5 text-body ${dark ? 'text-mist hover:text-white' : 'text-ink-2 hover:text-ink'}`}>{t.nav.how}</a></li>
            <li><a href={`/${locale}#safe`} className={`rounded-lg px-3 py-2.5 text-body ${dark ? 'text-mist hover:text-white' : 'text-ink-2 hover:text-ink'}`}>{t.nav.safe}</a></li>
            <li><Link href={`/${locale}/pool`} className={`rounded-lg px-3 py-2.5 text-body ${dark ? 'text-mist hover:text-white' : 'text-ink-2 hover:text-ink'}`}>{t.nav.pool}</Link></li>
            <li><Link href={`/${locale}/me`} className={`rounded-lg px-3 py-2.5 text-body ${dark ? 'text-mist hover:text-white' : 'text-ink-2 hover:text-ink'}`}>{t.nav.me}</Link></li>
          </ul>
          <div className="flex items-center rounded-lg ring-1 ring-current/20" role="group" aria-label={t.nav.language}>
            {locales.map((l) => (
              <Link key={l} href={`/${l}${rest}`} hrefLang={l} lang={l} aria-current={l === locale ? 'true' : undefined} className={`inline-flex min-h-11 min-w-11 items-center justify-center rounded-lg px-2.5 text-caption font-bold ${l === locale ? (dark ? 'bg-white/15 text-white' : 'bg-ink text-white') : dark ? 'text-mist hover:text-white' : 'text-ink-2 hover:text-ink'}`}>
                <abbr title={localeName[l]} className="no-underline">{l.toUpperCase()}</abbr>
              </Link>
            ))}
          </div>
        </div>
      </nav>
    </header>
  );
}
