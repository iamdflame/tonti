'use client';

import { Nav } from '@/components/Nav';
import { useI18n } from '@/i18n/client';

/** "This page went dark. Her income won't." One lamp still lit over the bay. */
export default function NotFound() {
  const { t, locale } = useI18n();
  return (
    <>
      <Nav />
      <main id="main" tabIndex={-1} className="outline-none dusk flex min-h-dvh items-center bg-bay text-foreground">
        <div className="mx-auto w-full max-w-2xl px-4 py-24 md:px-8">
          <span className="block size-4 rounded-full bg-lamp shadow-[0_0_28px_8px_rgba(255,178,63,0.45)]" aria-hidden="true" />
          <h1 className="mt-10 text-title font-extrabold text-balance text-white">{t.notFound.title}</h1>
          <p className="mt-4 max-w-[48ch] text-body-l text-mist">{t.notFound.body}</p>
          <a href={`/${locale}#ask`} className="press mt-10 inline-flex min-h-14 items-center justify-center rounded-xl bg-lamp px-6 text-body-l font-extrabold text-ink hover:bg-lamp-soft">
            {t.notFound.home}
          </a>
        </div>
      </main>
    </>
  );
}
