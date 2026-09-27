'use client';

import { createContext, useContext, type ReactNode } from 'react';
import { type Dict, type Locale, format } from '@/i18n';

const Ctx = createContext<{ locale: Locale; t: Dict } | null>(null);

export function I18nProvider({ locale, dict, children }: { locale: Locale; dict: Dict; children: ReactNode }) {
  return <Ctx.Provider value={{ locale, t: dict }}>{children}</Ctx.Provider>;
}

export function useI18n() {
  const c = useContext(Ctx);
  if (!c) throw new Error('useI18n outside I18nProvider');
  return { ...c, f: format };
}
