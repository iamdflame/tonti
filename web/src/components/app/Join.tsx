'use client';

import { useEffect, useMemo, useState } from 'react';
import { type Address, isAddress, zeroAddress } from 'viem';
import { countries } from '@/sdk/countries.ts';
import { memberIdFromLogs, type JoinInput } from '@/sdk/client.ts';
import type { Iso3, Sex } from '@/sdk/units.ts';
import { useI18n } from '@/i18n/client';
import { coreLive } from '@/lib/chain';
import { countryName, short } from '@/lib/format';
import { createLifeKey, passkeyFailure, passkeysAvailable } from '@/lib/passkey';
import { app, FROM_BLOCK } from '@/lib/app-chain';
import { Notice } from '@/components/Notice';
import { AddressInput, Card, Label, Page, Primary, Secondary, b64, noGas, reason } from './ui';
import { Account } from './Account';
import { PayIn } from './PayIn';
import { useWallet } from './Wallet';
import { OpenInBrowser, useInApp } from './InApp';

type Who = 'mother' | 'father' | 'me';
type Step = 'plan' | 'account' | 'key' | 'people' | 'confirm' | 'done' | 'invite' | 'waiting';
const YEAR = new Date().getUTCFullYear();
const BEQUEST = [0, 2_000, 5_000] as const;

/** Joining: for oneself (life key on this phone), or for a parent (an invite: her life key must be
 * made on her own phone, since it is what proves she is alive). */
export function Join() {
  const { t, f, locale } = useI18n();
  const w = useWallet();
  const [step, setStep] = useState<Step>('plan');
  const [who, setWho] = useState<Who>('mother');
  const [meSex, setMeSex] = useState<Sex>('female');
  const [country, setCountry] = useState<Iso3>('PHL');
  const [born, setBorn] = useState(1966);
  const [startAge, setStartAge] = useState(62);
  const [escalating, setEscalating] = useState(false);
  const [beta, setBeta] = useState<number>(0);
  const [key, setKey] = useState<{ qx: `0x${string}`; qy: `0x${string}` } | null>(null);
  const [payout, setPayout] = useState('');
  const [heir, setHeir] = useState('');
  const [guardians, setGuardians] = useState(['', '', '']);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [keyErr, setKeyErr] = useState<string | null>(null);
  const inApp = useInApp();
  const [memberId, setMemberId] = useState<bigint | null>(null);
  const [name, setName] = useState('');
  const [herLang, setHerLang] = useState<'fil' | 'en'>('fil');

  useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    const wq = p.get('w');
    if (wq === 'mother' || wq === 'father' || wq === 'me') setWho(wq);
    if (p.get('s') === 'm') setMeSex('male');
    const c = p.get('c');
    if (c && countries.some((x) => x.iso3 === c)) setCountry(c as Iso3);
    if (p.get('b')) setBorn(Number(p.get('b')));
    if (p.get('a')) setStartAge(Number(p.get('a')));
    if (p.get('e') === '1') setEscalating(true);
    const q = Number(p.get('q'));
    if ((BEQUEST as readonly number[]).includes(q)) setBeta(q);
  }, []);
  useEffect(() => {
    if (w.sender && !payout) setPayout(w.sender.address);
    if (w.sender && !heir) setHeir(w.sender.address);
  }, [w.sender, payout, heir]);

  const sex: Sex = who === 'mother' ? 'female' : who === 'father' ? 'male' : meSex;
  const minStart = Math.max(50, Math.ceil(YEAR - born + 0.5));
  const fitted = countries.find((x) => x.iso3 === country)?.birthYears ?? [YEAR - 80, YEAR - 18];
  const joinInput = (): JoinInput => ({
    country,
    sex,
    birthYear: born,
    startAge,
    bequestShare: beta / 10_000,
    escalating,
    payout: payout as Address,
    beneficiary: heir as Address,
    passkey: key!,
    guardians: guardians.map((g) => (isAddress(g) ? g : zeroAddress)) as [Address, Address, Address],
  });

  const invite = useMemo(() => {
    if (!w.sender) return '';
    const payload = b64.enc({ v: 1, c: country, s: sex === 'female' ? 'f' : 'm', b: born, a: startAge, e: escalating ? 1 : 0, q: beta, n: name.trim().slice(0, 40), g: w.sender.address });
    return `${typeof window !== 'undefined' ? window.location.origin : ''}/${herLang}/i#${payload}`;
  }, [w.sender, country, sex, born, startAge, escalating, beta, name, herLang]);

  // A parent joins from her own phone and names this account her first guardian: watch for it.
  useEffect(() => {
    if (step !== 'waiting' || !w.sender || !coreLive) return;
    let stop = false;
    const tick = async () => {
      const mine = await app.accountsOf(w.sender!.address, FROM_BLOCK).catch(() => []);
      const theirs = mine.filter((a) => a.roles.includes('guardian')).at(-1);
      if (theirs && !stop) {
        setMemberId(theirs.memberId);
        setStep('done');
      } else if (!stop) setTimeout(tick, 6000);
    };
    tick();
    return () => {
      stop = true;
    };
  }, [step, w.sender]);

  const join = async () => {
    setBusy(true);
    setErr(null);
    try {
      const mined = await w.sender!.send([app.calls.join(joinInput())]);
      setMemberId(memberIdFromLogs(mined.flatMap((m) => m.logs)));
      setStep('done');
    } catch (e) {
      setErr(noGas(e) ? f(t.app.noGasFor, { address: short(w.sender!.address) }) : reason(e));
    } finally {
      setBusy(false);
    }
  };

  const planLink = () => `${window.location.origin}/${locale}/join?w=me&s=${sex === 'male' ? 'm' : 'f'}&c=${country}&b=${born}&a=${startAge}&e=${escalating ? 1 : 0}&q=${beta}`;

  const steps: Step[] = who === 'me' ? ['plan', 'account', 'key', 'people', 'confirm'] : ['plan', 'account', 'invite'];
  const at = steps.indexOf(step);
  const stepName: Record<Step, string> = { plan: t.join.stepPlan, account: t.join.stepAccount, key: t.join.stepKey, people: t.join.stepPeople, confirm: t.join.stepConfirm, invite: t.join.stepInvite, waiting: t.join.stepInvite, done: t.join.stepConfirm };
  const next = () => setStep(steps[Math.min(steps.length - 1, at + 1)]);
  const back = () => setStep(steps[Math.max(0, at - 1)]);
  const radio = (on: boolean) => `press flex min-h-12 cursor-pointer items-center justify-center rounded-xl px-2 text-center text-body font-bold ring-1 ${on ? 'bg-ink text-white ring-ink' : 'bg-paper ring-ink/20 hover:bg-ink/5'}`;

  return (
    <Page title={t.join.title}>
      {!coreLive && <Notice>{t.app.notLive}</Notice>}
      {at >= 0 && step !== 'done' && (
        <ol className="flex gap-2" aria-label={t.join.title}>
          {steps.map((s, i) => (
            <li key={s} aria-current={i === at ? 'step' : undefined} className={`h-1.5 flex-1 rounded-full ${i <= at ? 'bg-ink' : 'bg-ink/15'}`}>
              <span className="sr-only">{stepName[s]}</span>
            </li>
          ))}
        </ol>
      )}

      {who === 'me' && inApp !== null && !key && (step === 'plan' || step === 'account' || step === 'key') && <OpenInBrowser app={inApp} url={planLink()} wallet />}

      {step === 'plan' && (
        <Card>
          <div className="space-y-6">
            <fieldset>
              <legend className="mb-2 text-body font-bold">{t.join.forWho}</legend>
              <div className="grid grid-cols-3 gap-2">
                {(['mother', 'father', 'me'] as const).map((x) => (
                  <label key={x} className={radio(who === x)}>
                    <input type="radio" className="sr-only" name="who" checked={who === x} onChange={() => setWho(x)} />
                    {t.form[x]}
                  </label>
                ))}
              </div>
            </fieldset>
            {who === 'me' && (
              <fieldset>
                <legend className="mb-2 text-body font-bold">{t.form.sex}</legend>
                <div className="grid grid-cols-2 gap-2">
                  {(['female', 'male'] as const).map((x) => (
                    <label key={x} className={radio(meSex === x)}>
                      <input type="radio" className="sr-only" name="sex" checked={meSex === x} onChange={() => setMeSex(x)} />
                      {x === 'female' ? t.form.woman : t.form.man}
                    </label>
                  ))}
                </div>
              </fieldset>
            )}
            <div className="grid gap-5 sm:grid-cols-3">
              <div>
                <Label htmlFor="country">{who === 'me' ? t.form.countryMe : t.form.country}</Label>
                <select id="country" value={country} onChange={(e) => setCountry(e.target.value as Iso3)} className="h-12 w-full rounded-xl bg-haze px-3 text-body ring-1 ring-input">
                  {[...countries].sort((a, b) => countryName(locale, a.iso3, a.name).localeCompare(countryName(locale, b.iso3, b.name))).map((c) => (
                    <option key={c.iso3} value={c.iso3}>{countryName(locale, c.iso3, c.name)}</option>
                  ))}
                </select>
              </div>
              <div>
                <Label htmlFor="born">{t.form.born}</Label>
                <input id="born" inputMode="numeric" value={born} onChange={(e) => setBorn(Number(e.target.value.replace(/\D/g, '')) || 0)} className="h-12 w-full rounded-xl bg-haze px-3 font-mono text-body ring-1 ring-input" />
              </div>
              <div>
                <Label htmlFor="start" hint={f(t.form.startAgeHint, { min: minStart })}>{t.form.startAge}</Label>
                <input id="start" inputMode="numeric" value={startAge} onChange={(e) => setStartAge(Number(e.target.value.replace(/\D/g, '')) || 0)} className="h-12 w-full rounded-xl bg-haze px-3 font-mono text-body ring-1 ring-input" />
              </div>
            </div>
            <fieldset>
              <legend className="mb-2 text-body font-bold">{t.join.plan}</legend>
              <div className="grid gap-2 sm:grid-cols-2">
                {[false, true].map((x) => (
                  <label key={String(x)} className={`${radio(escalating === x)} flex-col !items-start !justify-start gap-0.5 p-3 text-left`}>
                    <input type="radio" className="sr-only" name="plan" checked={escalating === x} onChange={() => setEscalating(x)} />
                    <span>{x ? t.join.rising : t.join.level}</span>
                    <span className={`text-caption font-normal ${escalating === x ? 'text-white/80' : 'text-ink-2'}`}>{x ? t.join.risingHint : t.join.levelHint}</span>
                  </label>
                ))}
              </div>
            </fieldset>
            <fieldset>
              <legend className="text-body font-bold">{t.join.bequest}</legend>
              <p className="mt-0.5 mb-2 text-caption text-ink-2">{t.join.bequestHint}</p>
              <div className="grid grid-cols-3 gap-2">
                {BEQUEST.map((x) => (
                  <label key={x} className={radio(beta === x)}>
                    <input type="radio" className="sr-only" name="beta" checked={beta === x} onChange={() => setBeta(x)} />
                    {x / 100}%
                  </label>
                ))}
              </div>
            </fieldset>
            <Primary onClick={next} disabled={born < Math.max(YEAR - 79, fitted[0]) || born > Math.min(YEAR - 18, fitted[1]) || startAge < minStart || startAge > 80}>{t.app.next}</Primary>
          </div>
        </Card>
      )}

      {step === 'account' && (
        <Card>
          <div className="space-y-5">
            <Account />
            <div className="flex gap-3">
              <Secondary onClick={back}>{t.app.back}</Secondary>
              <Primary onClick={next} disabled={!w.sender}>{t.app.next}</Primary>
            </div>
          </div>
        </Card>
      )}

      {step === 'key' && (
        <Card>
          <div className="space-y-4">
            <h2 className="text-heading font-bold">{t.join.lifeKeyTitle}</h2>
            <p className="text-body text-ink-2">{t.join.lifeKeyBody}</p>
            {!passkeysAvailable() ? (
              <p role="alert" className="text-body text-danger">{t.join.noPasskeys}</p>
            ) : key ? (
              <p className="text-body font-bold text-ok">✓ {t.join.lifeKeyDone}</p>
            ) : (
              <Primary busy={busy} onClick={async () => {
                setKeyErr(null);
                setBusy(true);
                try {
                  const k = await createLifeKey('Tonti life key');
                  setKey({ qx: k.qx, qy: k.qy });
                } catch (e) {
                  const why = passkeyFailure(e);
                  setKeyErr(why === 'blocked' ? t.join.keyBlocked : why === 'cancelled' ? t.join.keyCancelled : f(t.app.failed, { detail: reason(e) }));
                } finally {
                  setBusy(false);
                }
              }}>{t.join.lifeKeyButton}</Primary>
            )}
            {keyErr && !key && <p role="alert" className="text-body text-danger">{keyErr}</p>}
            <div className="flex gap-3">
              <Secondary onClick={back}>{t.app.back}</Secondary>
              <Primary onClick={next} disabled={!key}>{t.app.next}</Primary>
            </div>
          </div>
        </Card>
      )}

      {step === 'people' && (
        <Card>
          <div className="space-y-6">
            <AddressInput id="payout" label={t.join.payout} hint={t.join.payoutHint} value={payout} onChange={setPayout} />
            <AddressInput id="heir" label={t.join.beneficiary} hint={t.join.beneficiaryHint} value={heir} onChange={setHeir} />
            <fieldset className="space-y-3">
              <legend className="text-body font-bold">{t.join.guardians}</legend>
              <p className="text-caption text-ink-2">{t.join.guardiansHint}</p>
              {guardians.map((g, i) => (
                <input key={i} value={g} onChange={(e) => setGuardians((gs) => gs.map((x, j) => (j === i ? e.target.value.trim() : x)))} placeholder="0x…" aria-label={`${t.join.guardians} ${i + 1}`} spellCheck={false}
                  className="h-12 w-full rounded-xl bg-haze px-3 font-mono text-body ring-1 ring-input" />
              ))}
            </fieldset>
            <div className="flex gap-3">
              <Secondary onClick={back}>{t.app.back}</Secondary>
              <Primary onClick={next} disabled={!isAddress(payout) || !isAddress(heir)}>{t.app.next}</Primary>
            </div>
          </div>
        </Card>
      )}

      {step === 'confirm' && (
        <Card>
          <div className="space-y-5">
            <h2 className="text-heading font-bold">{t.join.confirmTitle}</h2>
            <dl className="grid gap-2 text-body">
              <div className="flex justify-between gap-4"><dt className="text-ink-2">{t.form.countryMe}</dt><dd>{countryName(locale, country, countries.find((c) => c.iso3 === country)?.name ?? country)}</dd></div>
              <div className="flex justify-between gap-4"><dt className="text-ink-2">{t.form.born}</dt><dd className="font-mono">{born}</dd></div>
              <div className="flex justify-between gap-4"><dt className="text-ink-2">{t.form.startAge}</dt><dd className="font-mono">{startAge}</dd></div>
              <div className="flex justify-between gap-4"><dt className="text-ink-2">{t.join.plan}</dt><dd>{escalating ? t.join.rising : t.join.level}</dd></div>
              <div className="flex justify-between gap-4"><dt className="text-ink-2">{t.join.bequest}</dt><dd className="font-mono">{beta / 100}%</dd></div>
            </dl>
            <Account />
            {err && <p role="alert" className="text-body text-danger">{f(t.app.failed, { detail: err })}</p>}
            <div className="flex gap-3">
              <Secondary onClick={back}>{t.app.back}</Secondary>
              <Primary busy={busy} onClick={join} disabled={!coreLive}>{busy ? t.app.working : t.join.join}</Primary>
            </div>
          </div>
        </Card>
      )}

      {step === 'invite' && (
        <Card>
          <div className="space-y-5">
            <h2 className="text-heading font-bold">{t.join.inviteTitle}</h2>
            <p className="text-body text-ink-2">{t.join.inviteBody}</p>
            <div>
              <Label htmlFor="name">{t.join.yourName}</Label>
              <input id="name" value={name} onChange={(e) => setName(e.target.value)} autoComplete="given-name" className="h-12 w-full rounded-xl bg-haze px-3 text-body ring-1 ring-input" />
            </div>
            <fieldset>
              <legend className="mb-2 text-body font-bold">{t.join.herLanguage}</legend>
              <div className="grid grid-cols-2 gap-2">
                {(['fil', 'en'] as const).map((l) => (
                  <label key={l} className={radio(herLang === l)}>
                    <input type="radio" className="sr-only" name="lang" checked={herLang === l} onChange={() => setHerLang(l)} />
                    {l === 'fil' ? 'Filipino' : 'English'}
                  </label>
                ))}
              </div>
            </fieldset>
            <p className="text-caption text-ink-2">{t.join.guardianNote}</p>
            <ShareLink url={invite} label={t.join.shareInvite} copy={t.join.copyInvite} copied={t.join.copied} />
            <div className="flex gap-3">
              <Secondary onClick={back}>{t.app.back}</Secondary>
              <Primary onClick={() => setStep('waiting')} disabled={!coreLive}>{t.app.next}</Primary>
            </div>
          </div>
        </Card>
      )}

      {step === 'waiting' && (
        <Card>
          <p className="text-body-l" role="status">{t.join.waiting}</p>
        </Card>
      )}

      {step === 'done' && memberId !== null && (
        <>
          <Card>
            <div className="space-y-3">
              <p className="text-heading font-bold">{who === 'me' ? f(t.join.joined, { id: String(memberId) }) : f(t.join.herJoined, { id: String(memberId) })}</p>
              <p className="text-body text-ink-2">{t.join.identityNext}</p>
              {process.env.NEXT_PUBLIC_OPERATOR_CONTACT && (
                <a href={process.env.NEXT_PUBLIC_OPERATOR_CONTACT} className="press inline-flex min-h-12 items-center rounded-xl px-4 font-bold ring-1 ring-ink/20">{t.join.identityContact}</a>
              )}
              <a href={`/${locale}/checkin/${memberId}`} className="inline-flex min-h-11 items-center text-body underline underline-offset-4">{t.checkin.title}</a>
            </div>
          </Card>
          <PayIn memberId={memberId} />
        </>
      )}
    </Page>
  );
}

function ShareLink({ url, label, copy, copied }: { url: string; label: string; copy: string; copied: string }) {
  const [done, setDone] = useState(false);
  return (
    <div className="space-y-3">
      <div className="rounded-xl bg-haze p-3 font-mono text-caption break-all text-ink-2">{url}</div>
      <div className="grid gap-2 sm:grid-cols-2">
        <Primary onClick={() => (navigator.share ? navigator.share({ url }).catch(() => undefined) : window.open(`https://wa.me/?text=${encodeURIComponent(url)}`, '_blank'))}>{label}</Primary>
        <Secondary onClick={async () => { await navigator.clipboard.writeText(url); setDone(true); setTimeout(() => setDone(false), 1600); }}>{done ? copied : copy}</Secondary>
      </div>
    </div>
  );
}
