'use client';

import { useState, useSyncExternalStore } from 'react';
import { Check, Copy } from 'lucide-react';
import { useI18n } from '@/i18n/client';
import { inAppBrowser } from '@/lib/passkey';
import { Notice } from '@/components/Notice';

const never = () => () => {};

/** The app whose built-in browser this is ('' unnamed), or null in a real browser (and on the server). */
export function useInApp() {
  return useSyncExternalStore(never, inAppBrowser, () => null);
}

/** Said before anyone taps: a life key can't be made or used in a wallet's or a social app's
 * browser. Hands over the link (with the plan in it) to open in Safari or Chrome. */
export function OpenInBrowser({ app, url, use, wallet }: { app: string; url?: string; use?: boolean; wallet?: boolean }) {
  const { t, f } = useI18n();
  const [copied, setCopied] = useState(false);
  const link = url ?? (typeof window !== 'undefined' ? window.location.href : '');
  const where = app ? f(t.join.appBrowser, { app }) : t.join.thisAppBrowser;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(link);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Some web views refuse the clipboard: the link is on screen to select.
    }
  };
  return (
    <Notice kind="hold">
      <p className="font-bold">{t.join.inAppTitle}</p>
      <p className="mt-1">{f(use ? t.join.inAppCheckIn : t.join.inApp, { app: where })}</p>
      {wallet && <p className="mt-2">{f(t.join.inAppWallet, { signIn: t.app.signIn })}</p>}
      <button type="button" onClick={copy} className="press mt-3 inline-flex min-h-12 items-center gap-2 rounded-xl bg-ink px-4 font-bold text-white">
        {copied ? <Check className="size-5" aria-hidden="true" /> : <Copy className="size-5" aria-hidden="true" />}
        {copied ? t.result.copied : t.join.copyLink}
      </button>
      <p className="mt-2 font-mono text-caption break-all text-ink-2 select-all">{link}</p>
    </Notice>
  );
}
