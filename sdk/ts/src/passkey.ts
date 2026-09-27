/**
 * Passkey (WebAuthn P-256) helpers for the LifeRegistry: enrolment public keys and check-in
 * assertions, in exactly the shape `LifeRegistry.verifyWebAuthn` checks on-chain.
 */
import { type Hex, bytesToHex, hexToBytes } from 'viem';

/** P-256 group order, for low-s normalisation (the verifier rejects high s). */
const N = 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;

export type WebAuthnAuth = {
  authenticatorData: Hex;
  clientDataJSON: string;
  challengeIndex: bigint;
  typeIndex: bigint;
  r: Hex;
  s: Hex;
};

/** The bytes to pass as `publicKey.challenge` to `navigator.credentials.get` for a check-in. */
export function challengeBytes(challenge: Hex): Uint8Array {
  return hexToBytes(challenge);
}

/** (qx, qy) for `join` from a credential's SubjectPublicKeyInfo (`response.getPublicKey()`). */
export function publicKeyFromSpki(spki: Uint8Array): { qx: Hex; qy: Hex } {
  // An uncompressed P-256 point is the last 65 bytes: 0x04 ‖ X ‖ Y.
  const point = spki.slice(spki.length - 65);
  if (point[0] !== 0x04) throw new Error('not an uncompressed P-256 public key');
  return { qx: bytesToHex(point.slice(1, 33)), qy: bytesToHex(point.slice(33, 65)) };
}

/** Splits an ASN.1 DER ECDSA signature into 32-byte r and s. */
export function derToRs(der: Uint8Array): { r: bigint; s: bigint } {
  let i = 0;
  const expect = (b: number) => {
    if (der[i++] !== b) throw new Error('bad DER signature');
  };
  const len = () => {
    const l = der[i++];
    if (l & 0x80) throw new Error('unexpected long-form DER length');
    return l;
  };
  expect(0x30);
  len();
  expect(0x02);
  const rl = len();
  const r = BigInt(bytesToHex(der.slice(i, i + rl)));
  i += rl;
  expect(0x02);
  const sl = len();
  const s = BigInt(bytesToHex(der.slice(i, i + sl)));
  return { r, s };
}

const word = (x: bigint): Hex => `0x${x.toString(16).padStart(64, '0')}`;

/**
 * Converts an assertion (`navigator.credentials.get(...).response`) into the struct
 * `checkIn(memberId, auth)` takes. s is normalised to the low half, as the verifier requires.
 */
export function toWebAuthnAuth(a: { authenticatorData: Uint8Array; clientDataJSON: Uint8Array; signature: Uint8Array }): WebAuthnAuth {
  const json = new TextDecoder().decode(a.clientDataJSON);
  const challengeIndex = json.indexOf('"challenge":"');
  const typeIndex = json.indexOf('"type":"webauthn.get"');
  if (challengeIndex < 0 || typeIndex < 0) throw new Error('not a webauthn.get assertion');
  const { r, s } = derToRs(a.signature);
  return {
    authenticatorData: bytesToHex(a.authenticatorData),
    clientDataJSON: json,
    // Offsets are in bytes: the JSON is ASCII up to these fields in every browser.
    challengeIndex: BigInt(new TextEncoder().encode(json.slice(0, challengeIndex)).length),
    typeIndex: BigInt(new TextEncoder().encode(json.slice(0, typeIndex)).length),
    r: word(r),
    s: word(s > N / 2n ? N - s : s),
  };
}
