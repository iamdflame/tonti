'use client';

import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { isAddress } from 'viem';
import { useI18n } from '@/i18n/client';

export function Page({ title, lead, children }: { title: string; lead?: string; children: ReactNode }) {
  return (
    <div className="mx-auto max-w-2xl px-4 pt-10 md:px-8 md:pt-14">
      <h1 className="text-title font-extrabold">{title}</h1>
      {lead && <p className="mt-3 text-body-l text-ink-2">{lead}</p>}
      <div className="mt-8 space-y-6">{children}</div>
    </div>
  );
}

/** A step's card. `locked` is a step that opens later: dashed and unfilled, but its text keeps full
 * contrast (fading it with opacity would drop the words below 4.5:1). */
export function Card({ children, className, locked }: { children: ReactNode; className?: string; locked?: boolean }) {
  const skin = locked ? 'border-2 border-dashed border-ink/20 bg-transparent' : 'bg-paper shadow-sm';
  return <section className={`rounded-2xl p-5 sm:p-6 ${skin} ${className ?? ''}`}>{children}</section>;
}

export function Primary({ children, busy, ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { busy?: boolean }) {
  return (
    <button {...p} disabled={p.disabled || busy} aria-busy={busy} className={`press inline-flex min-h-14 w-full items-center justify-center gap-2 rounded-xl bg-lamp px-5 text-body-l font-extrabold text-ink hover:bg-lamp-soft disabled:cursor-not-allowed disabled:opacity-60 disabled:not-aria-busy:bg-haze-2 disabled:not-aria-busy:text-ink-2 disabled:not-aria-busy:opacity-100 ${p.className ?? ''}`}>
      {children}
    </button>
  );
}

export function Secondary({ children, ...p }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button {...p} className={`press inline-flex min-h-12 items-center justify-center gap-2 rounded-xl px-4 font-bold text-ink ring-1 ring-ink/20 hover:bg-ink/5 disabled:opacity-50 ${p.className ?? ''}`}>
      {children}
    </button>
  );
}

export function Label({ htmlFor, children, hint }: { htmlFor: string; children: ReactNode; hint?: string }) {
  return (
    <div className="mb-2">
      <label htmlFor={htmlFor} className="block text-body font-bold">
        {children}
      </label>
      {hint && <p className="mt-0.5 text-caption text-ink-2">{hint}</p>}
    </div>
  );
}

export function AddressInput({ id, value, onChange, label, hint }: { id: string; value: string; onChange(v: string): void; label: string; hint?: string }) {
  const { t } = useI18n();
  const bad = value !== '' && !isAddress(value);
  return (
    <div>
      <Label htmlFor={id} hint={hint}>
        {label}
      </Label>
      <input id={id} value={value} onChange={(e) => onChange(e.target.value.trim())} placeholder="0x…" spellCheck={false} autoComplete="off" aria-invalid={bad} aria-describedby={bad ? `${id}-err` : undefined}
        className="h-12 w-full rounded-xl bg-haze px-3 font-mono text-body ring-1 ring-input outline-none focus-visible:ring-2 focus-visible:ring-ring aria-invalid:ring-danger" />
      {bad && <p id={`${id}-err`} role="alert" className="mt-1.5 text-caption text-danger">{t.app.addressInvalid}</p>}
    </div>
  );
}


export const b64 = {
  enc: (o: unknown) => btoa(unescape(encodeURIComponent(JSON.stringify(o)))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, ''),
  dec: <T,>(s: string): T | null => {
    try {
      return JSON.parse(decodeURIComponent(escape(atob(s.replace(/-/g, '+').replace(/_/g, '/')))));
    } catch {
      return null;
    }
  },
};

/** Reverts come back as the contract's own error names; show those, not a stack. */
export function reason(e: unknown): string {
  const m = (e as { shortMessage?: string; message?: string })?.shortMessage ?? (e as Error)?.message ?? String(e);
  const named = /reverted with the following reason:\s*(.+)|Error: (\w+)\(\)|error (\w+)/i.exec(m);
  return (named?.[1] ?? named?.[2] ?? named?.[3] ?? m).slice(0, 180);
}
