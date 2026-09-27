# Tonti SDK

A typed TypeScript client for the frontend. It's built on viem, and its ABIs are generated from the contracts, so it can't drift from them.

```ts
import { createPublicClient, http } from 'viem';
import { tonti, countries } from '@tonti/sdk';
import deployment from '../config/deployment.json'; // written by deploy/deploy.sh

const pool = tonti(createPublicClient({ transport: http('https://rpc.mainnet.chain.robinhood.com') }), deployment);

// The gasp test. The Actuary runs a 512-path Monte Carlo on-chain inside an eth_call.
const q = await pool.quote({ country: 'PHL', sex: 'female', birthYear: 1966, startAge: 62, lumpSum: 3000, monthly: 30 });
q.incomeStart.p50;          // ≈ 26.2 dollars a month, for life
q.soloRunoutAge;            // ≈ 80.8: the same income drawn alone runs out here…
q.soloOutliveProbability;   // ≈ 0.49 …and this is the chance she's still alive then
```

**What's in it**

| | |
|---|---|
| `countries` | The 12 countries the Actuary prices, with the birth years fitted (1935–2005), for the frontend's pickers. |
| `quote` | Human units in and out: dollars, ages, probabilities. |
| `join`, then the identity check, then `contribute` | `join` returns the new member id. The operator checks the member's document and signs with `bin/attest.ts`, and `strongProof` submits it (`identified` turns true). Only then does `contribute` accept money, approving USDG to the pool if needed; anyone may pay for anyone. No money goes in that couldn't come out. |
| `claim`, `requestExit`, `cancelExit`, `exit`, `claimable` | The member's money: income, notice, exit, estates. |
| `member`, `owed`, `value`, `state` | Status pages: units, owed income, value at oracle prices, the settlement phase, pause. |
| `identified`, `held` | Whether the identity check is done, and whether the ghost detector holds the member's group until they prove who they are again. |
| `attestIdentity`, `bin/attest.ts` | The operator's side of the identity check. The command refuses to sign unless the document's birth year, sex and country give exactly the member's on-chain cohort key. |
| `publicKeyFromSpki`, `challenge`, `toWebAuthnAuth`, `checkIn` | Passkeys. Enrol from `response.getPublicKey()`. For a check-in, sign `challengeBytes(await pool.challenge(id))` with `navigator.credentials.get`, convert the response, and submit it. |
| `settle` | For keepers: pushes settlement pages until the epoch is done. |

**Checks**
- `npm run typecheck` type-checks every call against the generated ABIs.
- `npm test` includes an end-to-end passkey test. A real P-256 key signs a WebAuthn assertion, the SDK converts it, and the real `LifeRegistry` bytecode on a local anvil chain must accept it. It must also reject a wrong challenge, tampered client data, and an un-normalised high-s signature.

**Regenerating the ABIs** (after any contract change): `python3 sdk/gen.py`. It runs `cargo stylus export-abi` for the Stylus contracts, adds their events, and compiles everything with Foundry.
