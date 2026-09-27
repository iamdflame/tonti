'use client';

import { useEffect, useState } from 'react';
import { memberIdFromLogs, type JoinInput } from '@/sdk/client.ts';
import type { WebAuthnAuth } from '@/sdk/passkey.ts';
import { useI18n } from '@/i18n/client';
import { app } from '@/lib/app-chain';
import { Card, Page, Primary, b64, reason } from './ui';
import { Account } from './Account';
import { useWallet } from './Wallet';

type Payload = { v: 1; t: 'join'; j: JoinInput } | { v: 1; t: 'checkin'; id: string; auth: Omit<WebAuthnAuth, 'challengeIndex' | 'typeIndex'> & { challengeIndex: string; typeIndex: string } };

/** A parent's joining or check-in, finished from a family member's wallet. Both calls are open to
 * anyone: they count only because the parent's own life key signed or registered them. */
export function Relay() {
  const { t, f } = useI18n();
  const w = useWallet();
  const [p, setP] = useState<Payload | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => setP(b64.dec<Payload>(window.location.hash.slice(1))), []);
  if (p === undefined) return null;
  if (!p || p.v !== 1) return <Page title={t.relay.title}><p role="alert">{t.invite.broken}</p></Page>;

  const send = async () => {
    setBusy(true);
    setErr(null);
    try {
      if (p.t === 'join') {
        const mined = await w.sender!.send([app.calls.join(p.j)]);
        setDone(f(t.relay.sent, { id: String(memberIdFromLogs(mined.flatMap((m) => m.logs))) }));
      } else {
        const auth: WebAuthnAuth = { ...p.auth, challengeIndex: BigInt(p.auth.challengeIndex), typeIndex: BigInt(p.auth.typeIndex) };
        await w.sender!.send([app.calls.checkIn(BigInt(p.id), auth)]);
        setDone(t.relay.checkedIn);
      }
    } catch (e) {
      setErr(reason(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Page title={t.relay.title} lead={p.t === 'join' ? t.relay.joinBody : t.relay.checkinBody}>
      <Card>
        <div className="space-y-5">
          <Account />
          {done ? (
            <p role="status" className="text-heading font-bold text-ok">{done}</p>
          ) : (
            <Primary busy={busy} onClick={send} disabled={!w.sender}>{busy ? t.app.working : t.relay.send}</Primary>
          )}
          {err && <p role="alert" className="text-body text-danger">{f(t.app.failed, { detail: err })}</p>}
        </div>
      </Card>
    </Page>
  );
}
