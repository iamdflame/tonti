import { intlLocale, type Locale } from '@/i18n';

export const usd = (locale: Locale, v: number, digits = 2) =>
  new Intl.NumberFormat(intlLocale[locale], { style: 'currency', currency: 'USD', minimumFractionDigits: digits, maximumFractionDigits: digits }).format(v);

export const money = (locale: Locale, v: number, currency: string) =>
  new Intl.NumberFormat(intlLocale[locale], { style: 'currency', currency, maximumFractionDigits: v >= 1000 ? 0 : 2 }).format(v);

export const num = (locale: Locale, v: number | bigint, digits = 0) =>
  new Intl.NumberFormat(intlLocale[locale], { maximumFractionDigits: digits }).format(v);

export const pct = (locale: Locale, v: number) => new Intl.NumberFormat(intlLocale[locale], { style: 'percent', maximumFractionDigits: 0 }).format(v);

export const date = (locale: Locale, d: Date) => new Intl.DateTimeFormat(intlLocale[locale], { dateStyle: 'long' }).format(d);

export const short = (a: string) => `${a.slice(0, 6)}…${a.slice(-4)}`;

/** Each fitted country's currency, where the ECB publishes a rate (others show dollars only). */
export const CURRENCY: Record<string, string | undefined> = { PHL: 'PHP', IDN: 'IDR', IND: 'INR', MYS: 'MYR', THA: 'THB', CHN: 'CNY', SGP: 'SGD' };

const ISO2: Record<string, string> = { PHL: 'PH', IDN: 'ID', IND: 'IN', BGD: 'BD', MMR: 'MM', LKA: 'LK', NPL: 'NP', VNM: 'VN', THA: 'TH', MYS: 'MY', CHN: 'CN', SGP: 'SG' };
const names = new Map<Locale, Intl.DisplayNames>();
/** A fitted country's name in the reader's language (Pilipinas, not Philippines, on /fil). */
export const countryName = (locale: Locale, iso3: string, fallback: string) => {
  const iso2 = ISO2[iso3];
  if (!iso2) return fallback;
  if (!names.has(locale)) names.set(locale, new Intl.DisplayNames([intlLocale[locale]], { type: 'region' }));
  return (names.get(locale)!.of(iso2) ?? fallback).replace(/ \(.*\)$/, '');
};
