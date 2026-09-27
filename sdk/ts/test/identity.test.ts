/**
 * End to end: the SDK's EIP-712 identity attestation (viem) must be accepted by the real
 * AttestedIdentity + LifeRegistry (OpenZeppelin EIP712) on a local anvil chain. And the thing it
 * exists to stop must fail: an attestation for an older birth year than the member joined with.
 */
import assert from 'node:assert/strict';
import { type ChildProcess, execFileSync, spawn, spawnSync } from 'node:child_process';
import { chmodSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { generateKeyPairSync } from 'node:crypto';
import { after, before, test } from 'node:test';
import { type Hex, createPublicClient, createWalletClient, http } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { foundry } from 'viem/chains';
import { attestIdentity, attestedIdentityAbi, cohortKey, lifeRegistryAbi, publicKeyFromSpki, recoveryAction } from '../src/index.ts';

const CONTRACTS = new URL('../../../contracts/', import.meta.url).pathname;
const PORT = 9545 + Math.floor(Math.random() * 1000);
const transport = http(`http://127.0.0.1:${PORT}`);
// anvil's default test accounts 0 and 1 (well-known keys, never used on a real chain)
const deployer = privateKeyToAccount('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80');
const attester = privateKeyToAccount('0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d');
const client = createPublicClient({ chain: foundry, transport });
const wallet = createWalletClient({ chain: foundry, transport, account: deployer });
const attesterWallet = createWalletClient({ chain: foundry, transport, account: attester });
let anvil: ChildProcess;

const deploy = async (name: string, abi: readonly unknown[], args: readonly unknown[]) => {
  const bytecode = execFileSync('forge', ['inspect', name, 'bytecode'], { cwd: CONTRACTS }).toString().trim() as Hex;
  const hash = await wallet.deployContract({ abi, bytecode, args } as never);
  return (await client.waitForTransactionReceipt({ hash })).contractAddress as Hex;
};

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
});
after(() => anvil?.kill());

test('an SDK-signed identity attestation identifies the member; one for an older birth year fails', async () => {
  const registry = await deploy('LifeRegistry', lifeRegistryAbi, ['0x000000000000000000000000000000000000dEaD']);
  const verifier = await deploy('AttestedIdentity', attestedIdentityAbi, [attester.address, registry]);
  const send = async (functionName: string, args: readonly unknown[]) => {
    // Simulate first, so a revert fails the test with the contract's own error.
    await client.simulateContract({ address: registry, abi: lifeRegistryAbi, functionName, args, account: deployer } as never);
    const hash = await wallet.writeContract({ address: registry, abi: lifeRegistryAbi, functionName, args } as never);
    return client.waitForTransactionReceipt({ hash });
  };
  await send('setVerifier', [verifier, true]); // during set-up (no pool yet) a verifier is usable at once
  await send('setPool', [deployer.address]); // stands in for the pool contract: it only calls enroll
  const key = cohortKey('PHL', 'female', 1966);
  const { publicKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const { qx, qy } = publicKeyFromSpki(new Uint8Array(publicKey.export({ format: 'der', type: 'spki' })));
  const zero = '0x0000000000000000000000000000000000000000';
  await send('enroll', [1n, key, qx, qy, deployer.address, deployer.address, [zero, zero, zero]]);
  const can = () => client.readContract({ address: registry, abi: lifeRegistryAbi, functionName: 'canReceiveIncome', args: [1n] });
  assert.equal(await can(), false, 'joined, not identified');

  // Claiming to be born in 1946 (older, so paid more) is refused.
  const older = await attestIdentity(attesterWallet, { verifier, registry, memberId: 1n, key: cohortKey('PHL', 'female', 1946) });
  await assert.rejects(client.simulateContract({ address: registry, abi: lifeRegistryAbi, functionName: 'strongProof', args: [1n, verifier, older], account: deployer }), /BadSignature/);

  const proof = await attestIdentity(attesterWallet, { verifier, registry, memberId: 1n, key });
  await send('strongProof', [1n, verifier, proof]);
  assert.equal(await can(), true, 'identified and alive');
  // The same statement can't be submitted twice (it would stretch the identity period).
  await assert.rejects(client.simulateContract({ address: registry, abi: lifeRegistryAbi, functionName: 'strongProof', args: [1n, verifier, proof], account: deployer }), /BadSignature/);

  // A lost phone: a statement bound to the new wallet and passkey moves the account; a plain
  // identification can't.
  const { publicKey: newKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const next = publicKeyFromSpki(new Uint8Array(newKey.export({ format: 'der', type: 'spki' })));
  const newWallet = '0x000000000000000000000000000000000000c0DE';
  const plain = await attestIdentity(attesterWallet, { verifier, registry, memberId: 1n, key, issuedAt: Math.floor(Date.now() / 1000) - 30 });
  await assert.rejects(client.simulateContract({ address: registry, abi: lifeRegistryAbi, functionName: 'recover', args: [1n, newWallet, next.qx, next.qy, verifier, plain], account: deployer }), /BadSignature/);
  const bound = await attestIdentity(attesterWallet, { verifier, registry, memberId: 1n, key, action: recoveryAction(newWallet, next.qx, next.qy), issuedAt: Math.floor(Date.now() / 1000) - 30 });
  await send('recover', [1n, newWallet, next.qx, next.qy, verifier, bound]);
  // Nothing moves for the challenge window (a check-in with the old passkey would cancel it); then anyone applies it.
  const read = (functionName: string) => client.readContract({ address: registry, abi: lifeRegistryAbi, functionName, args: [1n] } as never) as Promise<{ payout: string; qx: string }>;
  assert.notEqual((await read('life')).payout.toLowerCase(), newWallet.toLowerCase(), 'not before the delay');
  await assert.rejects(client.simulateContract({ address: registry, abi: lifeRegistryAbi, functionName: 'finishRecovery', args: [1n], account: deployer }), /waiting/);
  // The chain is shared with the next test: jump the window inside a snapshot, then come back.
  const snapshot = await client.request({ method: 'evm_snapshot', params: [] } as never);
  const window = Number(await client.readContract({ address: registry, abi: lifeRegistryAbi, functionName: 'challengeWindow' }));
  await client.request({ method: 'evm_increaseTime', params: [window] } as never);
  await client.request({ method: 'evm_mine', params: [] } as never);
  await send('finishRecovery', [1n]);
  const life = await read('life');
  assert.equal(life.payout.toLowerCase(), newWallet.toLowerCase());
  assert.equal(life.qx, next.qx);
  await client.request({ method: 'evm_revert', params: [snapshot] } as never);
});

test("the attester's command refuses a document that doesn't match the member's cohort", async () => {
  const registry = await deploy('LifeRegistry', lifeRegistryAbi, ['0x000000000000000000000000000000000000dEaD']);
  const verifier = await deploy('AttestedIdentity', attestedIdentityAbi, [attester.address, registry]);
  const send = async (functionName: string, args: readonly unknown[]) => {
    // Simulate first, so a revert fails the test with the contract's own error.
    await client.simulateContract({ address: registry, abi: lifeRegistryAbi, functionName, args, account: deployer } as never);
    const hash = await wallet.writeContract({ address: registry, abi: lifeRegistryAbi, functionName, args } as never);
    return client.waitForTransactionReceipt({ hash });
  };
  await send('setVerifier', [verifier, true]);
  await send('setPool', [deployer.address]);
  const { publicKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const { qx, qy } = publicKeyFromSpki(new Uint8Array(publicKey.export({ format: 'der', type: 'spki' })));
  const zero = '0x0000000000000000000000000000000000000000';
  await send('enroll', [5n, cohortKey('IDN', 'male', 1958), qx, qy, deployer.address, deployer.address, [zero, zero, zero]]);

  const dir = mkdtempSync(join(tmpdir(), 'tonti-attest-'));
  const keyFile = join(dir, 'attester.key');
  writeFileSync(keyFile, '0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d');
  chmodSync(keyFile, 0o600);
  writeFileSync(join(dir, 'deployment.json'), JSON.stringify({ lifeRegistry: registry, attestedIdentity: verifier }));
  const run = (born: string) =>
    spawnSync('node', ['bin/attest.ts', '--member', '5', '--country', 'IDN', '--sex', 'male', '--born', born, '--key-file', keyFile,
      '--deployment', join(dir, 'deployment.json'), '--rpc', `http://127.0.0.1:${PORT}`, '--submit'], { cwd: new URL('..', import.meta.url).pathname, encoding: 'utf8' });

  const lie = run('1948'); // ten years older than the cohort the member joined
  assert.notEqual(lie.status, 0);
  assert.match(lie.stderr, /refusing to attest/);
  const ok = run('1958');
  assert.equal(ok.status, 0, ok.stderr);
  assert.match(ok.stdout, /submitted 0x[0-9a-f]{64}: success/);
  assert.equal(await client.readContract({ address: registry, abi: lifeRegistryAbi, functionName: 'canReceiveIncome', args: [5n] }), true);
});
