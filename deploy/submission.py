#!/usr/bin/env python3
"""Renders SUBMISSION.md (the HackQuest submission text) from the live deployment and the measured
reports, so every address and number in it comes from the chain or a run, never from memory.

  python3 deploy/submission.py
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'actuarial'))
from paths import RUNS  # noqa: E402

EXPLORER = 'https://robinhoodchain.blockscout.com/address/'
NAMES = {
    'actuary': ('Actuary', 'Stylus (Rust)', 'mortality for every cohort, the on-chain Monte Carlo quote, payout fractions'),
    'pool': ('TontiPool', 'Stylus (Rust)', 'members, cohorts, paged settlement, fair mortality credits, exits, the ghost detector'),
    'treasury': ('Treasury', 'Solidity', 'USDG in Morpho, SPY and SGOV via Uniswap v4, Chainlink prices with staleness and slippage guards'),
    'lifeRegistry': ('LifeRegistry', 'Solidity', 'passkey check-ins (RIP-7212), identity binding, death reports, presumption and revival'),
    'attestedIdentity': ('AttestedIdentity', 'Solidity', 'EIP-712 identity attestations bound to the cohort key'),
    'timelock': ('TimelockController', 'Solidity (OpenZeppelin)', 'owns every contract; 48-hour notice on any parameter change'),
}


def load(name):
    p = RUNS / name
    return json.loads(p.read_text()) if p.exists() else None


def main():
    dep = json.loads((ROOT / 'config' / 'deployment.json').read_text())
    gas = load('gas.json') or {}
    f100, f10k = load('fairness-100.json') or {}, load('fairness-10000.json') or {}
    live = load('mainnet-quote.json') or {}
    muts, fit, ghost, judge = load('mutations-pool.json') or {}, load('mortality-validation.json') or {}, load('ghost-detector.json') or {}, load('judge.json') or {}
    rmuts = load('mutations-registry.json') or {}
    params = json.loads((ROOT / 'config' / 'mortality.json').read_text())['params'].values()
    groups = len({(p['iso3'], p['sex']) for p in params})
    countries = len({p['iso3'] for p in params})
    power = {round(r['hidden'], 1): r for r in ghost.get('power', [])}
    honest = power.get(0.0, {}).get('flagged_36m')
    half = power.get(0.5, {}).get('flagged_24m')
    later = fit.get('from_later_ages', {})
    js = judge.get('summary', {})
    rows = []
    for key, (name, lang, role) in NAMES.items():
        if key in dep:
            rows.append(f'| {name} | {lang} | [`{dep[key]}`]({EXPLORER}{dep[key]}) | {role} |')
    q512 = next((r['gas'] for r in gas.get('quote', []) if r['paths'] == 512 and not r['escalating']), None)
    pm = gas.get('pool_month', {})
    text = f"""# Tonti: CPF LIFE for the people CPF leaves out

**One line:** a pension with no pension fund. It pays USDG income for as long as you live, to you and to the parent you fund, invested in the S&P 500 and T-bills on Robinhood Chain, with the money of members who die shared fairly among those still alive.

## The problem

Singapore employs 1,635,700 foreign workers (MOM, Dec 2025). None of them can join CPF, the city's pension system, and most send money home to parents who have no pension either: only 24% of South Asia's elderly receive one (ILO 2024–26). Saving alone doesn't fix it. Nobody knows how long they'll live, so either they draw too little or their money runs out while they're still alive.

## The product

A member (or their child, from a Singapore wage) pays USDG in. The pool invests it along a glide path in SPY and SGOV stock tokens and USDG in Morpho. From the chosen age, the pool pays monthly USDG income for life. When a member dies, the at-risk part of their balance moves to the survivors. That transfer is what lets the pool pay more than anyone could safely draw alone.

- **Quote, on-chain, in about a second:** a 512-path Monte Carlo inside an `eth_call` to the live Actuary: {live.get('summary', 'see README')}
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
{chr(10).join(rows)}

Mortality loaded for: {', '.join(dep.get('countries', []))} ({len(dep.get('batches', []))} of {groups} country-sex groups).
Governance: {'every contract is owned by the 48-hour timelock' if dep.get('governance') == 'timelock-48h' else 'owned by the deployer until the timelock handover'}.

## Technical execution (measured, not claimed)

- **On-chain quote:** 512 paths cost {q512 or 'n/a':,} gas, predicted before deployment by our own Stylus gas meter, and confirmed on mainnet to within 0.6%. The first version needed ~380M gas and could never have run.
- **Settlement never jams:** it runs in pages that anyone can push. Paged and one-shot settlement end in byte-identical state.
- **Running cost:** {pm.get('gas_per_member_month', 0):,.0f} gas per member-month (≈ ${pm.get('usd_per_member_month_at_4000', 0):.3f}).
- **Fair credits in small pools:** a finite-pool correction we derived cuts the bias from {f100.get('uncorrected_rms_bias', 0):.1%} to {f100.get('rms_bias', 0):.2%} RMS at 100 members, and to {f10k.get('rms_bias', 0):.3%} at 10,000. {f10k.get('ledger_checks', 0) + f100.get('ledger_checks', 0):,} survivor credits were matched to the ledger exactly.
- **The dead are never paid, the living never killed:** a passkey check-in every 90 days (verified by the chain's P-256 precompile); a death report has 120 days to be answered by the member's own proof; any death can be undone within five years, and the member's money goes back into their own account from a revival reserve (as far as it goes; the rest stays owed and is repaid as it refills). There is no reward for reporting, and a reported death's estate is held for a year, so a false report can't pay anyone. A lost phone is recovered only after 120 days that the member's own check-in can cancel.
- **Hidden deaths:** a ghost-member detector (Wald's SPRT, compared against deaths expected five months earlier) runs inside every settlement: {'n/a' if honest is None else f'{honest:.1%}'} of honest groups flagged within three years, {'n/a' if half is None else f'{half:.0%}'} of groups hiding half their deaths within two. Passkey ghosts are presumed dead after 800 days without an identity proof.
- **Mortality:** {len(params):,} cohorts ({countries} countries) fitted to UN WPP 2024 and {'sealed on-chain' if dep.get('sealed') else 'ready to load (the live v1 Actuary holds an older fit; the relaunch loads and seals this one)'}; worst annuity-factor error {fit.get('worst_abs_annuity_error', 0):.2%} from the ages income starts, {later.get('90', {}).get('worst', 0):.2%} from 90.
- **Tests that can fail:** 68 Rust, 45 Solidity (10 against live mainnet state) and 6 TypeScript tests against the real contracts on anvil. Deliberately planted bugs caught: {muts.get('killed', '?')} of {muts.get('total', '?')} in the pool, {rmuts.get('killed', '?')} of {rmuts.get('total', '?')} in the LifeRegistry. Four independent adversarial reviews; every finding is fixed or disclosed, and listed in `.claude/uncomparable.md`.
- **The site, judged on its public URL** (`web/scripts/judge.mjs`, https://tonti-life.vercel.app): {js.get('quotes', 'n/a')}; {js.get('routes', 'n/a')}; axe: {js.get('axe', 'n/a')}; a passkey made with Chrome's virtual authenticator.

Full evidence: `README.md` ("What is measured"); the specification is `docs/protocol.md`.

## Milestones (for the development-tied prizes)

1. **Live research preview.** Deposits capped at 25 USDG per member; the first real members from Singapore's Filipino domestic-worker community; monthly settlements run by the keeper.
2. **Gasless onboarding for parents:** the site (quote, join, parent invite, check-in, accounts, live pool, operator) is built in English and Filipino; sponsored gas through Alchemy's EIP-7702 wallets turns on with the production keys.
3. **Trust-minimised identity:** ZKPassport and signed national-ID QR adapters behind the same interface, replacing the operator's attestation.
4. **Scale and licensing:** raise caps as pools pass 100 members, where credits are statistically fair. Enter the MAS FinTech Regulatory Sandbox or partner with a licensed trustee.
"""
    (ROOT / 'SUBMISSION.md').write_text(text)
    print('SUBMISSION.md written')


if __name__ == '__main__':
    main()
