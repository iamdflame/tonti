import { en, type Dict } from './en';
import { fil } from './fil';

export const locales = ['en', 'fil'] as const;
export type Locale = (typeof locales)[number];
export const defaultLocale: Locale = 'en';
/** BCP 47 tags for Intl formatting. */
export const intlLocale: Record<Locale, string> = { en: 'en-SG', fil: 'fil-PH' };
export const localeName: Record<Locale, string> = { en: 'English', fil: 'Filipino' };

const dictionaries: Record<Locale, Dict> = { en, fil };

export const hasLocale = (l: string): l is Locale => (locales as readonly string[]).includes(l);
export const getDictionary = (l: Locale): Dict => dictionaries[l];

/** "{amount} a month" with {amount} → value. Whole sentences only: never glue strings together. */
export function format(template: string, vars: Record<string, string | number> = {}): string {
  return template.replace(/\{(\w+)\}/g, (_, k: string) => (k in vars ? String(vars[k]) : `{${k}}`));
}

export type { Dict };
