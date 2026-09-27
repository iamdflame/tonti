'use client';

import { useEffect, useState } from 'react';
import { useI18n } from '@/i18n/client';
import { blockNumber } from '@/lib/rpc';
import { num } from '@/lib/format';

/** The latest Robinhood Chain block, live: the page is talking to the chain, not a mock. */
export function ChainPulse({ className }: { className?: string }) {
  const { t, f, locale } = useI18n();
  const [block, setBlock] = useState<bigint | null>(null);
  useEffect(() => {
    let stop = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      if (!document.hidden) {
        try {
          const b = await blockNumber();
          if (!stop) setBlock(b);
        } catch {
          if (!stop) setBlock(null);
        }
      }
      timer = setTimeout(tick, 4000);
    };
    tick();
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  }, []);
  return (
    <p className={`inline-flex items-center gap-2 text-caption text-muted-foreground ${className ?? ''}`} aria-live="off">
      <span className={`size-2 rounded-full ${block ? 'bg-lamp' : 'bg-mist'}`} aria-hidden="true" />
      <span className="font-mono tabular">{block ? f(t.pulse.live, { block: num(locale, block) }) : t.pulse.offline}</span>
    </p>
  );
}
