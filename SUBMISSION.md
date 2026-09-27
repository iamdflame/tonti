# Tonti: CPF LIFE for the people CPF leaves out

**One line:** a pension with no pension fund. It pays USDG income for as long as you live, to you and to the parent you fund, invested in the S&P 500 and T-bills on Robinhood Chain, with the money of members who die shared fairly among those still alive.

## The problem

Singapore employs 1,635,700 foreign workers (MOM, Dec 2025). None of them can join CPF, the city's pension system, and most send money home to parents who have no pension either: only 24% of South Asia's elderly receive one (ILO 2024–26). Saving alone doesn't fix it. Nobody knows how long they'll live, so either they draw too little or their money runs out while they're still alive.

## The product

A member (or their child, from a Singapore wage) pays USDG in. The pool invests it along a glide path in SPY and SGOV stock tokens and USDG in Morpho. From the chosen age, the pool pays monthly USDG income for life. When a member dies, the at-risk part of their balance moves to the survivors. That transfer is what lets the pool pay more than anyone could safely draw alone.

- **Quote, on-chain, in about a second:** a 512-path Monte Carlo inside an `eth_call` to the live Actuary: a female born 1966 in PHL with $3,000 + $30/month gets $25.28/month for life from 62 (P10 $22.89, P90 $27.71); drawn alone the same income runs out at 80.4, with a 50% chance of still being alive (1.0 s, 9,522,504 gas)
- **Level or Escalating plans,** like CPF LIFE's.
- **Exit before income starts,** with 12 months' notice.
- **Nobody can claim to be older than they are:** deposits, income and exits need an identity statement bound to the member's birth year, sex and country.

## Why on-chain, and why Robinhood Chain

- **Why a contract:** by 1905, two-thirds of US life insurance was tontine insurance. New York banned it in 1906 after the Armstrong Investigation found the insurers embezzling. The operator was the failure mode, and here the rules are code that no operator can take the pot from.
- **Why Robinhood Chain:** its SPY and SGOV tokens give real equity and T-bill exposure.
- **Why USDG:** all money in and out is Paxos USDG, and the cash sleeve earns in Morpho's USDG vault.

## Deployed on Robinhood Chain mainnet (chain 4663)

| Contract | Language | Address | Role |
|---|---|---|---|
| Actuary | Stylus (Rust) | [`0x4cd86134f0ec64263df710f0414d21a18d116525`](https://robinhoodchain.blockscout.com/address/0x4cd86134f0ec64263df710f0414d21a18d116525) | mortality for every cohort, the on-chain Monte Carlo quote, payout fractions |

Mortality loaded for: PHL, IDN, IND, BGD, MMR, LKA, NPL, VNM, THA, MYS, CHN, SGP (24 of 24 country-sex groups).
Governance: owned by the deployer until the timelock handover.

## Technical execution (measured, not claimed)

- **On-chain quote:** 512 paths cost 9,458,297 gas, predicted before deployment by our own Stylus gas meter, and confirmed on mainnet to within 0.6%. The first version needed ~380M gas and could never have run.
- **Settlement never jams:** it runs in pages that anyone can push. Paged and one-shot settlement end in byte-identical state.
- **Running cost:** 301,968 gas per member-month (≈ $0.033).
- **Fair credits in small pools:** a finite-pool correction we derived cuts the bias from 4.8% to 0.26% RMS at 100 members, and to 0.012% at 10,000. 201,211,945 survivor credits were matched to the ledger exactly.
- **The dead are never paid, the living never killed:** a passkey check-in every 90 days (verified by the chain's P-256 precompile); a death report has 120 days to be answered by the member's own proof; any death can be undone within five years, and the member's money goes back into their own account from a revival reserve (as far as it goes; the rest stays owed and is repaid as it refills). There is no reward for reporting, and a reported death's estate is held for a year, so a false report can't pay anyone. A lost phone is recovered only after 120 days that the member's own check-in can cancel.
- **Hidden deaths:** a ghost-member detector (Wald's SPRT, compared against deaths expected five months earlier) runs inside every settlement: 0.0% of honest groups flagged within three years, 83% of groups hiding half their deaths within two. Passkey ghosts are presumed dead after 800 days without an identity proof.
- **Mortality:** 1,704 cohorts fitted to UN WPP 2024 and ready to load (the live v1 Actuary holds an older fit; the relaunch loads and seals this one); worst annuity-factor error 1.35% from the ages income starts, 1.70% from 90.
- **Tests that can fail:** 62 Rust, 39 Solidity (9 against live mainnet state) and 6 TypeScript tests against the real contracts on anvil. Deliberately planted bugs caught: 26 of 26 in the pool, 15 of 15 in the LifeRegistry. Three independent adversarial reviews; every finding is fixed or disclosed, and listed in `.claude/uncomparable.md`.
- **The site, judged on its public URL** (`web/scripts/judge.mjs`, https://tonti-life.vercel.app): 20/20 equal a direct eth_call and encode the inputs asked; 80/80 route×width×language loads clean; axe: 0 violation types over 20 pages; a passkey made with Chrome's virtual authenticator.

Full evidence: `README.md` ("What is measured"); the specification is `docs/protocol.md`.

## Milestones (for the development-tied prizes)

1. **Live research preview.** Deposits capped at 25 USDG per member; the first real members from Singapore's Filipino domestic-worker community; monthly settlements run by the keeper.
2. **Gasless onboarding for parents:** the site (quote, join, parent invite, check-in, accounts, live pool, operator) is built in English and Filipino; sponsored gas through Alchemy's EIP-7702 wallets turns on with the production keys.
3. **Trust-minimised identity:** ZKPassport and signed national-ID QR adapters behind the same interface, replacing the operator's attestation.
4. **Scale and licensing:** raise caps as pools pass 100 members, where credits are statistically fair. Enter the MAS FinTech Regulatory Sandbox or partner with a licensed trustee.
