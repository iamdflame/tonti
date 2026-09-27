import { notFound } from 'next/navigation';
import { Hero } from '@/components/landing/Hero';
import { Lights } from '@/components/landing/Lights';
import { Cta, Day, Footer, Hood, Limits, Safety } from '@/components/landing/Sections';
import { Nav } from '@/components/Nav';
import { getDictionary, hasLocale } from '@/i18n';


export default async function Home({ params }: PageProps<'/[locale]'>) {
  const { locale } = await params;
  if (!hasLocale(locale)) notFound();
  const t = getDictionary(locale);
  return (
    <>
      <Nav />
      <main id="main" tabIndex={-1} className="outline-none">
        <Hero />
        <Lights />
        <Day t={t} />
        <Safety t={t} />
        <Hood t={t} locale={locale} />
        <Limits t={t} />
        <Cta t={t} locale={locale} />
      </main>
      <Footer t={t} />
    </>
  );
}
