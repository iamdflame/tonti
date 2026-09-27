import type { Hex } from 'viem';
import { publicKeyFromSpki, toWebAuthnAuth, type WebAuthnAuth } from '@/sdk/passkey.ts';

// The member's life key: a passkey on their own phone (Face ID or fingerprint). Its public key goes
// to the LifeRegistry at joining; every check-in is its signature over the registry's challenge.

const b64url = (b: ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(b as ArrayBuffer))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

export const passkeysAvailable = () => typeof window !== 'undefined' && !!window.PublicKeyCredential && !!navigator.credentials;

/** Creates the life key (ES256 only: the registry verifies P-256). */
export async function createLifeKey(label: string): Promise<{ qx: Hex; qy: Hex; credentialId: string }> {
  const cred = (await navigator.credentials.create({
    publicKey: {
      rp: { name: 'Tonti', id: location.hostname },
      user: { id: crypto.getRandomValues(new Uint8Array(16)), name: label, displayName: label },
      challenge: crypto.getRandomValues(new Uint8Array(32)),
      pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
      authenticatorSelection: { residentKey: 'required', userVerification: 'required' },
      timeout: 120_000,
    },
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error('no key was created');
  const res = cred.response as AuthenticatorAttestationResponse;
  const spki = res.getPublicKey();
  if (!spki) throw new Error('this browser did not share the public key');
  const { qx, qy } = publicKeyFromSpki(new Uint8Array(spki));
  const credentialId = b64url(cred.rawId);
  localStorage.setItem('tonti:life-key', credentialId);
  return { qx, qy, credentialId };
}

/** Signs a check-in challenge with the life key. */
export async function signCheckIn(challenge: Hex): Promise<WebAuthnAuth> {
  const bytes = new Uint8Array((challenge.slice(2).match(/.{2}/g) ?? []).map((h) => parseInt(h, 16)));
  const known = localStorage.getItem('tonti:life-key');
  const cred = (await navigator.credentials.get({
    publicKey: {
      rpId: location.hostname,
      challenge: bytes,
      userVerification: 'required',
      timeout: 120_000,
      ...(known ? { allowCredentials: [{ type: 'public-key' as const, id: Uint8Array.from(atob(known.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((known.length + 3) % 4)), (c) => c.charCodeAt(0)) }] } : {}),
    },
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error('no signature');
  const r = cred.response as AuthenticatorAssertionResponse;
  return toWebAuthnAuth({ authenticatorData: new Uint8Array(r.authenticatorData), clientDataJSON: new Uint8Array(r.clientDataJSON), signature: new Uint8Array(r.signature) });
}
