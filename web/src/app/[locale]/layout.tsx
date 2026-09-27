import type { Metadata, Viewport } from 'next';
import { Atkinson_Hyperlegible_Mono, Atkinson_Hyperlegible_Next } from 'next/font/google';
import { notFound } from 'next/navigation';
import { I18nProvider } from '@/i18n/client';
import { getDictionary, hasLocale, locales } from '@/i18n';
import '../globals.css';

const sans = Atkinson_Hyperlegible_Next({ subsets: ['latin', 'latin-ext'], variable: '--font-atkinson', display: 'swap' });
const mono = Atkinson_Hyperlegible_Mono({ subsets: ['latin'], variable: '--font-atkinson-mono', display: 'swap' });

export const generateStaticParams = () => locales.map((locale) => ({ locale }));

export async function generateMetadata({ params }: LayoutProps<'/[locale]'>): Promise<Metadata> {
  const { locale } = await params;
  if (!hasLocale(locale)) return {};
  const t = getDictionary(locale);
  return {
    metadataBase: new URL(process.env.NEXT_PUBLIC_SITE_URL ?? 'https://tonti-life.vercel.app'),
    title: t.meta.title,
    description: t.meta.description,
    alternates: { languages: { en: '/en', fil: '/fil' } as Record<string, string> },
    openGraph: { title: t.meta.title, description: t.meta.description, type: 'website', locale: locale === 'fil' ? 'fil_PH' : 'en_SG' },
    twitter: { card: 'summary_large_image' },
    manifest: '/manifest.webmanifest',
    icons: { icon: '/icon.svg', apple: '/apple-icon.png' },
  };
}

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
  viewportFit: 'cover',
  themeColor: '#13233f',
};

export default async function LocaleLayout({ children, params }: LayoutProps<'/[locale]'>) {
  const { locale } = await params;
  if (!hasLocale(locale)) notFound();
  return (
    <html lang={locale} className={`${sans.variable} ${mono.variable}`}>
      <body className="min-h-dvh bg-background antialiased">
        <I18nProvider locale={locale} dict={getDictionary(locale)}>
          {children}
        </I18nProvider>
      </body>
    </html>
  );
}
