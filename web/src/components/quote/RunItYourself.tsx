'use client';

import { useState } from 'react';
import { decodeFunctionResult, type Address, type Hex } from 'viem';
import { call as ethCall, estimateGas } from 'viem/actions';
import { Check, Copy } from 'lucide-react';
import { actuaryAbi } from '@/sdk/abi.ts';
import { useI18n } from '@/i18n/client';
import { CALL_GAS_CAP, PUBLIC_RPC, explorer, publicOnly } from '@/lib/chain';
import { num, pct, short } from '@/lib/format';

/** The exact eth_call behind a quote, and a button that asks a node we don't run for the same
 * answer. Nothing on the page is precomputed; this is how anyone can check. */
export function RunItYourself({ call, p50Raw }: { call: { to: Address; data: Hex }; p50Raw: bigint }) {
  const { t, f, locale } = useI18n();
  const [state, setState] = useState<'idle' | 'asking' | 'same' | { diff: string }>('idle');
  const [gas, setGas] = useState<bigint | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const cast = `cast call ${call.to} ${call.data} --rpc-url ${PUBLIC_RPC}`;

  const rerun = async () => {
    setState('asking');
    try {
      const [out, g] = await Promise.all([ethCall(publicOnly, { to: call.to, data: call.data }), estimateGas(publicOnly, { to: call.to, data: call.data } as never).catch(() => null)]);
      const r = decodeFunctionResult({ abi: actuaryAbi, functionName: 'quote', data: out.data! }) as readonly bigint[];
      setGas(g);
      setState(r[1] === p50Raw ? 'same' : { diff: `P50 ${r[1]} vs ${p50Raw}` });
    } catch (e) {
      setState({ diff: (e as Error).message.slice(0, 120) });
    }
  };
  const copy = async (text: string, what: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(what);
    setTimeout(() => setCopied(null), 1600);
  };

  return (
    <details className="group rounded-xl bg-bay/60 ring-1 ring-white/10 open:bg-bay/80">
      <summary className="flex min-h-12 cursor-pointer list-none items-center justify-between gap-3 px-4 py-3 text-body font-bold text-foreground marker:hidden">
        {t.result.runIt}
        <span aria-hidden="true" className="text-mist transition-transform duration-200 group-open:rotate-45">+</span>
      </summary>
      <div className="space-y-4 px-4 pb-4 text-caption text-mist">
        <p>{t.result.runItLead}</p>
        <dl className="grid gap-2 font-mono text-[0.875rem] text-foreground/90">
          <div className="flex flex-wrap items-baseline gap-x-2">
            <dt className="text-mist">to</dt>
            <dd>
              <a className="underline decoration-white/30 underline-offset-4 hover:decoration-lamp" href={explorer('address', call.to)} target="_blank" rel="noreferrer">
                {short(call.to)}
              </a>
            </dd>
          </div>
          <div className="flex items-start gap-2">
            <dt className="text-mist">data</dt>
            <dd className="min-w-0 flex-1 break-all">{call.data.slice(0, 74)}…</dd>
            <button type="button" onClick={() => copy(call.data, 'data')} className="press inline-flex min-h-11 min-w-11 items-center justify-center rounded-lg text-mist hover:text-foreground" aria-label={t.result.copy}>
              {copied === 'data' ? <Check className="size-4" /> : <Copy className="size-4" />}
            </button>
          </div>
        </dl>
        <div className="flex items-center gap-2 rounded-lg bg-night/70 p-3 font-mono text-[0.8125rem] text-foreground/85">
          <code className="min-w-0 flex-1 truncate">{cast}</code>
          <button type="button" onClick={() => copy(cast, 'cast')} className="press inline-flex min-h-11 shrink-0 items-center gap-1.5 rounded-lg px-2 text-mist hover:text-foreground">
            {copied === 'cast' ? <Check className="size-4" /> : <Copy className="size-4" />}
            <span>{copied === 'cast' ? t.result.copied : t.result.copy}</span>
          </button>
        </div>
        <button
          type="button"
          onClick={rerun}
          disabled={state === 'asking'}
          className="press inline-flex min-h-12 w-full items-center justify-center rounded-xl bg-secondary px-4 font-bold text-foreground ring-1 ring-white/15 hover:bg-bay-3 disabled:opacity-70"
        >
          {state === 'asking' ? t.result.rerunning : t.result.rerun}
        </button>
        <p role="status" className="min-h-6 text-body text-foreground">
          {state === 'same' && (
            <span className="inline-flex items-center gap-2">
              <Check className="size-5 text-lamp" aria-hidden="true" />
              {t.result.same}
            </span>
          )}
          {typeof state === 'object' && f(t.result.different, { detail: state.diff })}
        </p>
        {gas !== null && <p className="font-mono tabular">{f(t.result.gas, { gas: num(locale, gas), share: pct(locale, Number(gas) / CALL_GAS_CAP) })}</p>}
      </div>
    </details>
  );
}
