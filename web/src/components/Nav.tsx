'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';
import { Menu, X } from 'lucide-react';
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
  // Phones get the same four links behind a menu button: without it, a member had no way back to
  // her account from a phone.
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!open) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('keydown', esc);
    return () => window.removeEventListener('keydown', esc);
  }, [open]);
  const links = [
    { href: `/${locale}/me`, label: t.nav.me },
    { href: `/${locale}#how`, label: t.nav.how },
    { href: `/${locale}#safe`, label: t.nav.safe },
    { href: `/${locale}/pool`, label: t.nav.pool },
  ];
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
          <button type="button" onClick={() => setOpen((x) => !x)} aria-expanded={open} aria-controls="site-menu" className={`press inline-flex min-h-11 min-w-11 items-center justify-center rounded-lg md:hidden ${dark ? 'text-white hover:bg-white/10' : 'text-ink hover:bg-ink/5'}`}>
            {open ? <X className="size-6" aria-hidden="true" /> : <Menu className="size-6" aria-hidden="true" />}
            <span className="sr-only">{open ? t.nav.closeMenu : t.nav.menu}</span>
          </button>
          <div className="flex items-center rounded-lg ring-1 ring-current/20" role="group" aria-label={t.nav.language}>
            {locales.map((l) => (
              <Link key={l} href={`/${l}${rest}`} hrefLang={l} lang={l} aria-current={l === locale ? 'true' : undefined} className={`inline-flex min-h-11 min-w-11 items-center justify-center rounded-lg px-2.5 text-caption font-bold ${l === locale ? (dark ? 'bg-white/15 text-white' : 'bg-ink text-white') : dark ? 'text-mist hover:text-white' : 'text-ink-2 hover:text-ink'}`}>
                <abbr title={localeName[l]} className="no-underline">{l.toUpperCase()}</abbr>
              </Link>
            ))}
          </div>
        </div>
      </nav>
      {open && (
        <div id="site-menu" className="absolute inset-x-0 top-full px-4 md:hidden">
          <ul className={`space-y-1 rounded-2xl p-2 shadow-lg ring-1 ${dark ? 'bg-night text-white ring-white/15' : 'bg-paper text-ink ring-ink/10'}`}>
            {links.map((l) => (
              <li key={l.href}>
                <a href={l.href} onClick={() => setOpen(false)} className={`flex min-h-12 items-center rounded-xl px-4 text-body-l font-bold ${dark ? 'hover:bg-white/10' : 'hover:bg-ink/5'}`}>{l.label}</a>
              </li>
            ))}
          </ul>
        </div>
      )}
    </header>
  );
}
