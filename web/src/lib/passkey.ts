import type { Hex } from 'viem';
import { publicKeyFromSpki, toWebAuthnAuth, type WebAuthnAuth } from '@/sdk/passkey.ts';

// The member's life key: a passkey on their own phone (Face ID or fingerprint). Its public key goes
// to the LifeRegistry at joining; every check-in is its signature over the registry's challenge.

const b64url = (b: ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(b as ArrayBuffer))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

export const passkeysAvailable = () => typeof window !== 'undefined' && !!window.PublicKeyCredential && !!navigator.credentials;

const IN_APP: [RegExp, string][] = [
  [/MetaMask/i, 'MetaMask'], [/CoinbaseWallet|CoinbaseBrowser/i, 'Coinbase Wallet'], [/Trust\//i, 'Trust Wallet'], [/OKApp/i, 'OKX'],
  [/BitKeep|Bitget/i, 'Bitget'], [/TokenPocket/i, 'TokenPocket'], [/Phantom/i, 'Phantom'], [/Rainbow/i, 'Rainbow'],
  [/MicroMessenger/i, 'WeChat'], [/FBAN|FBAV|FB_IAB/i, 'Facebook'], [/Instagram/i, 'Instagram'], [/\bLine\//i, 'LINE'],
  [/Telegram/i, 'Telegram'], [/musical_ly|Bytedance|TikTok/i, 'TikTok'], [/Snapchat/i, 'Snapchat'],
];

/** The app whose built-in browser this is ('' when it can't be named), or null in a real browser.
 * On a phone these web views can't make or use passkeys for other sites: iOS lets an embedded web
 * view use WebAuthn only for its own app's domains, and Android web views mostly lack it. The call
 * exists and is refused, so this has to be known before she taps. */
export function inAppBrowser(): string | null {
  if (typeof window === 'undefined') return null;
  const ua = navigator.userAgent;
  for (const [re, name] of IN_APP) if (re.test(ua)) return name;
  const w = window as unknown as { ReactNativeWebView?: unknown; ethereum?: { isMetaMask?: boolean } };
  if (w.ReactNativeWebView) return w.ethereum?.isMetaMask ? 'MetaMask' : '';
  if (/iPhone|iPad|iPod/.test(ua) && !/Safari\//.test(ua)) return '';
  if (/Android/.test(ua) && /; wv\)/.test(ua)) return '';
  return null;
}

/** Why a passkey call failed: the browser refused it, or she cancelled (or it timed out). */
export function passkeyFailure(e: unknown): 'blocked' | 'cancelled' | null {
  const name = (e as { name?: string })?.name;
  if (name === 'SecurityError' || (name === 'NotAllowedError' && inAppBrowser() !== null)) return 'blocked';
  if (name === 'NotAllowedError' || name === 'AbortError') return 'cancelled';
  return null;
}

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
