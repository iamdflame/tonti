/**
 * End to end: a real P-256 passkey signs a WebAuthn assertion, the SDK converts it, and the real
 * LifeRegistry bytecode (compiled by Foundry, run on a local anvil chain) must accept it, and
 * must reject tampering and un-normalised signatures.
 */
import assert from 'node:assert/strict';
import { type ChildProcess, execFileSync, spawn } from 'node:child_process';
import { createHash, generateKeyPairSync, sign } from 'node:crypto';
import { after, before, test } from 'node:test';
import { type Hex, createPublicClient, createWalletClient, http, keccak256, toHex } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { foundry } from 'viem/chains';
import { derToRs, lifeRegistryAbi, publicKeyFromSpki, toWebAuthnAuth } from '../src/index.ts';

const CONTRACTS = new URL('../../../contracts/', import.meta.url).pathname;
const PORT = 8545 + Math.floor(Math.random() * 1000);
const RPC = `http://127.0.0.1:${PORT}`;
// anvil's first default account (a well-known test key, never used on a real chain)
const account = privateKeyToAccount('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80');
const client = createPublicClient({ chain: foundry, transport: http(RPC) });
const wallet = createWalletClient({ chain: foundry, transport: http(RPC), account });
let anvil: ChildProcess;
let registry: Hex;

const sha256 = (b: Uint8Array) => new Uint8Array(createHash('sha256').update(b).digest());
const b64url = (b: Uint8Array) => Buffer.from(b).toString('base64url');
const N = 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;

function der(r: bigint, s: bigint): Uint8Array {
  const int = (x: bigint) => {
    let h = x.toString(16);
    if (h.length % 2) h = `0${h}`;
    let b = Buffer.from(h, 'hex');
    if (b[0] & 0x80) b = Buffer.concat([Buffer.from([0]), b]);
    return Buffer.concat([Buffer.from([0x02, b.length]), b]);
  };
  const body = Buffer.concat([int(r), int(s)]);
  return new Uint8Array(Buffer.concat([Buffer.from([0x30, body.length]), body]));
}

/** What a browser returns from navigator.credentials.get for this challenge. */
function assertion(privateKey: Parameters<typeof sign>[2], challenge: Hex) {
  const clientDataJSON = new TextEncoder().encode(
    JSON.stringify({ type: 'webauthn.get', challenge: b64url(Buffer.from(challenge.slice(2), 'hex')), origin: 'https://tonti.app', crossOrigin: false }),
  );
  const authenticatorData = new Uint8Array([...sha256(new TextEncoder().encode('tonti.app')), 0x05, 0, 0, 0, 1]);
  const signature = new Uint8Array(sign('sha256', Buffer.concat([authenticatorData, sha256(clientDataJSON)]), privateKey));
  return { authenticatorData, clientDataJSON, signature };
}

before(async () => {
  anvil = spawn('anvil', ['--port', String(PORT), '--silent'], { stdio: 'ignore' });
  for (let i = 0; i < 100; i++) {
    try {
      await client.getChainId();
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  const bytecode = execFileSync('forge', ['inspect', 'LifeRegistry', 'bytecode'], { cwd: CONTRACTS }).toString().trim() as Hex;
  const hash = await wallet.deployContract({ abi: lifeRegistryAbi, bytecode, args: ['0x000000000000000000000000000000000000dEaD'] });
  registry = (await client.waitForTransactionReceipt({ hash })).contractAddress as Hex;
});

after(() => anvil?.kill());

test("the SDK's check-in struct passes the real contract's WebAuthn verifier", async () => {
  const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const { qx, qy } = publicKeyFromSpki(new Uint8Array(publicKey.export({ format: 'der', type: 'spki' })));
  const challenge = await client.readContract({ address: registry, abi: lifeRegistryAbi, functionName: 'challenge', args: [7n] });
  const a = assertion(privateKey, challenge);
  const verify = (auth: ReturnType<typeof toWebAuthnAuth>, c: Hex = challenge) =>
    client.readContract({ address: registry, abi: lifeRegistryAbi, functionName: 'verifyWebAuthn', args: [c, auth, qx, qy] });

  assert.equal(await verify(toWebAuthnAuth(a)), true, 'a genuine assertion verifies');
  assert.equal(await verify(toWebAuthnAuth(a), keccak256(toHex('another member'))), false, 'bound to the challenge');
  const tampered = toWebAuthnAuth(a);
  tampered.clientDataJSON = tampered.clientDataJSON.replace('tonti.app', 'tonti.apq');
  assert.equal(await verify(tampered), false, 'clientDataJSON is covered by the signature');

  // The same signature with s in the high half: the SDK normalises it, the contract refuses raw high s.
  const { r, s } = derToRs(a.signature);
  const high = { ...a, signature: der(r, s > N / 2n ? s : N - s) };
  assert.equal(await verify(toWebAuthnAuth(high)), true, 'normalised high-s verifies');
  const raw = { ...toWebAuthnAuth(high), s: `0x${(s > N / 2n ? s : N - s).toString(16).padStart(64, '0')}` as Hex };
  assert.equal(await verify(raw), false, 'the contract rejects high s');
});
