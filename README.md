# Tonti: a pension with no pension fund

Singapore employs **1,635,700 foreign workers** (Ministry of Manpower, Dec 2025). None of them can contribute to CPF, the city's pension system, and most send money home to parents who have no pension either. Only 24% of South Asia's elderly receive one (ILO 2024–26).

Tonti pays **USDG income for as long as you live**: to you, or to the mother you fund from your wage. It invests in the S&P 500 and US Treasury bills on Robinhood Chain. When a member dies, what they had at risk is shared among the members still alive. That sharing is what lets it pay more than you could safely draw alone, for life, with no insurer in the middle.

**Ask it: [tonti-life.vercel.app](https://tonti-life.vercel.app)** (English and Filipino). Enter your mother's age, country and savings. A contract on Robinhood Chain simulates 512 possible lifetimes inside one `eth_call` and answers in about a second. "Run it yourself" shows the exact call and repeats it against a public node that we don't run.

## Why it has to be on-chain

This product has been tried before. By 1905, **two-thirds of all life insurance in force in the US** was tontine insurance. New York banned it in 1906, after the Armstrong Investigation found the insurers running it embezzling. People wanted what it offered; the companies running it were the failure. Here the rules are a contract:
- payouts are computed in public;
- credits follow a published formula whose fairness is measured;
- no operator can take the pot.

The pool can't go insolvent either, because it only redistributes money from deaths that actually happened.

## How it works

- **Cohorts, not accounts.** Members are grouped by birth year, sex, country, start age and plan. A cohort shares one mortality curve, one glide path and one payout rate, so a month's settlement costs O(cohorts), not O(members).
- **Mortality from the UN.** Each cohort has its own Gompertz–Makeham curve, fitted along its diagonal of the UN World Population Prospects 2024 tables. There are 1,704 cohorts: 12 countries, both sexes, born 1935–2005.
  - The fit targets what sets income: the annuity factor from every age income can start (50–80), and at half weight from 85, 90 and 95, where income is still paid.
  - Once the tables are loaded, the Actuary is **sealed**, and no one can change mortality again.
- **Fair mortality credits, even in small pools.**
  - When a member dies, their at-risk balance moves to the surviving cohorts **in kind** (as shares, with no trade).
  - It is weighted by `q/(1−q) × value` with a finite-pool correction, `× (1 + s − Σs²)`.
  - Plain odds-times-value weighting short-changes a small pool's biggest members. The correction, derived for this protocol, cancels that to first order.
- **Natural-tontine income.** Each month a paying cohort receives `1/ä` of its value, where ä is the annuity factor along its own survival curve. Only the pooled part pays income: a bequest share stays invested for the family and pays none, so income stays level instead of dwindling. The pool's 0.30% yearly fee is taken monthly, and the relaunched Actuary's quotes include it (the live v1 Actuary's don't yet, and the site says so).
- **Level or Escalating plans,** like CPF LIFE's. The Escalating plan is priced 2 points lower, so it starts about 20% lower and grows about 2% a year.
- **The glide path.** Until 40 the mix is 80% SPY, falling to 30% at 65. The rest is SGOV and USDG in Morpho. Opposite trades between cohorts cross inside the pool at the oracle price; only the net goes to market.
- **It can't be jammed.** Settlement runs in pages that anyone can push, at most N items per transaction. Paged and one-shot settlement give byte-identical state. A rebalance that can't trade (a paused token, say) can be abandoned after a day, so it never stops income.
- **Leaving before income starts.**
  - You give 12 months' notice and stay at risk during it.
  - Then you take 99% of your at-risk money and all your bequest share. The 1% stays with the members who remain.
  - A notice must be used within 90 days of maturing, and never after income has started.
- **Nobody can lie about their age.** Money goes in only after an identity check confirms your birth year, sex and country. Money comes out only with a fresh identity check at least every 400 days. Each identity statement is dated and counts once.
- **Proof of life, without paying the dead or killing the living:**
  - A passkey (Face ID or fingerprint, WebAuthn P-256) check-in every 90 days, verified on-chain by Robinhood Chain's RIP-7212 precompile. Alternatively, 2 of 3 guardians can vouch that you're alive, though never for who you are, and never to answer a death report.
  - Anyone can report a death with a bond. The member has **120 days** to answer with their own passkey or an identity proof, so a member who keeps their normal 90-day check-ins answers a false report without ever knowing of it. The date of death can't be set before the member's own last proof of life.
  - Any death, reported or presumed, can be **undone within five years** by an identity proof. The member's released money is repaid from a revival reserve **back into their account**, never paid out, so staging a death is no way to leave after income has started. The reserve repays as far as it goes; what it can't cover stays owed and is repaid as it refills, which in a young pool can take years. A later death clears any earlier revival. There is no reward for reporting a death, and a reported death's estate and bequest are **held for a year**: if the member is revived in that time they go back to the member, so a false report can't pay the family either.
  - Heirs who keep a dead member's passkey "alive" don't stop the clock: after 800 days without an identity proof, the member is presumed dead.
  - A statistical detector watches every country × birth-decade group for deaths that go unreported. Members of a flagged group get 120 days to renew their identity.
- **Accounts can change.** The payout address can move the income, the beneficiary and the guardians. A lost phone or wallet is recovered with an identity statement bound to the new wallet and passkey. It takes effect only after the challenge window (**120 days**, never less than a check-in period plus grace), income waits meanwhile, and one check-in with the old passkey cancels it. A member on her usual schedule always checks in first, so an identity statement alone can't take over a living member's account.

Full specification: [`docs/protocol.md`](docs/protocol.md).

## What is measured (not claimed)

| Claim | Evidence | Where |
|---|---|---|
| The quote really runs on-chain | A 512-path quote costs **9.5M gas**, 30% of Robinhood Chain's 32M per-call cap. This was predicted before deployment by our Stylus gas meter (nitro's own ink pricing) and matched on mainnet within 0.6%. The first version needed ~380M gas and could never have run | `engine/ink-meter`, the live site |
| The quote is right | It agrees with an independent float64 re-implementation to **1.1e-11** over 40 random quotes, with random markets, valuation rates, fees and path counts, and no run-out mismatches | `engine/tests/reference_quote.py` |
| Mortality matches the UN tables where it pays | Fitted to the annuity factor from every age income can start (50–80) and, at half weight, from 85, 90 and 95, because income keeps being paid at `1/ä` from the age reached. Against the UN tables it was fitted to: worst error **1.35%** from start ages (95th percentile 0.60%, median 0.16%, 7,680 cohort × age pairs); from 85, **1.30%**; from 90, **1.70%**; from 95, **3.74%**. Fitted from 50–80 only, it reached 2.2%, 4.7% and 7.3% from those ages (skeptic review 2) | `actuarial/validate_fit.py` |
| Credits are fair, even in small pools | On the same members and deaths, the correction cuts RMS bias from 4.8% to **0.26%** at 100 members, from 0.64% to 0.18% at 1,000, and from 0.051% to 0.012% at 10,000. At 20 members it is still 4.9% (from 31%) | `actuary-cli fairness` |
| Accounting can't leak | Exact share conservation over 10k random settlements and 10k random rebalances. A top-up never creates income, checked over 20,000 random holdings | `actuary-core`, `pool-stylus` tests |
| The tests can fail | The pool's test VM reverts on any call a test didn't set up, so paying the wrong address or enrolling the wrong key fails. Planted bugs, including every flaw the three independent reviews found: **26 in the pool** and **15 in the LifeRegistry**, each caught by the tests | `engine/tests/mutate_pool.py`, `engine/tests/mutate_registry.py` |
| Hidden deaths get caught, honest groups don't | Honest deaths take 4–6 months to become final, so each month is compared with the deaths expected 5 months earlier (testing each month against itself flagged **100%** of honest groups by month 3). A group of 3,300, 10 seeds × 300 runs: honest groups at the UN rates **0%** flagged within 3 years, 15% healthier 0.4%, 20% healthier 1.6%, 30% healthier 16%; 50% of deaths hidden, flagged 83% within 2 years and 99% within 3; 30% hidden, 17% within 3 (left to the yearly identity check). If honest deaths take 4–12 months to become final, 2.3% of honest groups are flagged | `engine/tests/ghost_power.py` |
| The dead are never paid, the living never killed | Real P-256 passkey signatures. Income is held while lapsed. A false report is answered by an on-schedule check-in, and the bond goes to the member. A final report can be undone within five years. Replayed or foreign identity statements are refused | `contracts/test/LifeRegistry.t.sol` |
| Trades are real and bounded | On live Robinhood Chain mainnet state, $1,000 → SPY → back costs 12.5 bps, and SGOV 9.5 bps. Stale prices and worse-than-oracle fills are refused. Members still see their value when feeds are stale (`lastPrices`) | `contracts/test/Treasury.fork.t.sol` |
| Running it is cheap | Settling and rebalancing a month for 300 members costs about **$0.03 per member-month**, in pages whose dearest used 5.8M gas, detection included | `engine/ink-meter` |
| It would have worked through history | 2,000 Philippine women retiring at 65 with $10,000 each, replayed through 1965, 1973 and 2000 (UN death rates, CRSP returns, CPI, measured trading costs). The pool's income drawn alone ran out with 48–54% of them still alive. That is by construction, since a pot paying 1/ä runs out near life expectancy; the pool paid every survivor for life | `actuarial/replay.py` |
| It fits on Robinhood Chain | Mainnet activation dry-runs pass: Actuary v2 34.8 KB, pool 53.3 KB | `cargo stylus check` |

**Tests:**
- Rust: 26 core, 10 Actuary, 26 pool.
- Solidity: 39, of which 9 run against live mainnet state.
- TypeScript: 6, against the real contracts on anvil.

Raw reports go to `runs/` (or `$TONTI_RUNS`).

## Governance, exactly

A 48-hour timelock owns every contract. The deployer proposes changes, and anyone can execute one once its notice has passed.

**What governance can change, within bounds set in code:**
- market assumptions (SPY return −5% to 15%, volatility 1% to 50%, safe rate 0% to 10%, pool fee 0% to 1%);
- the valuation rate, **2% to 5%**. It sets every member's payout fraction: across the whole band, income moves by about ±15%. That is the one lever governance has over payouts, and it waits 48 hours in public like every other change;
- escalation, which is never above the valuation rate;
- epoch length (28 to 31 days, so the detector's 5-month reporting lag always fits its 7-epoch ring), the member cap (up to 100,000 USDG) and the exit notice (180 to 730 days);
- check-in periods (the challenge window never shorter than a check-in period plus grace);
- trading routes, which must be the same token pair with no hooks;
- identity verifiers: a new one works only 30 days after it is allowed, while removal is immediate.

**What governance cannot touch:**
- mortality, which is sealed;
- member balances;
- payouts, beyond the valuation rate above;
- anyone's account.

The guardian can pause new business (joins and deposits). A pause holds settlements, rebalances and exits for **at most 7 days** from when it began, and nobody, the owner included, can pause more than once in 30 days. Claims, withdrawals, check-ins and death reports never stop.

**One key, four roles, in the research preview.** The deployer key proposes timelocked changes, is the pause guardian, and signs identity statements as the attester (the keeper should run with a separate key that holds no role). Each role is bounded as above, but that key is the preview's single point of trust. Before real money, the attester moves to a hardware key, and the proposer and guardian to a multisig.

## Layout

```
engine/actuary-core     no_std, float-free Rust: fixed point, mortality, ledger, credits, rebalance, quote, SPRT
engine/actuary-stylus   the Actuary contract: quote, q, payout fraction; sealed mortality; bounded market
engine/pool-stylus      the TontiPool contract: cohorts, members, paged settlement, claims, exits, revivals
engine/actuary-cli      native driver: precision, fairness, quotes, ghost-detector power, historical replay
engine/ink-meter        Stylus gas before deployment: the real WASM, metered as nitro meters it
engine/tests            precision, float reference, ghost-detector power, mutation testing
contracts/              Solidity: Treasury (Morpho, Uniswap v4, Chainlink), LifeRegistry, AttestedIdentity
actuarial/              UN WPP download, mortality fit and its validation, historical replay
sdk/                    typed TypeScript client; ABIs generated from the contracts
web/                    the site (Next.js): the live quote, joining, parent invites, check-ins, the pool, the operator desk
deploy/                 resumable mainnet deployment; services/ keeper and live quotes
```

## Run it

```bash
git submodule update --init                 # forge-std and OpenZeppelin v5.4.0
source engine/.cargo-env                     # builds go to RAM where /dev/shm exists
(cd engine && cargo test --release -p actuary-core -p actuary-stylus -p pool-stylus)
(cd engine && cargo build --release -p actuary-cli && python3 tests/reference_quote.py 40)
python3 engine/tests/mutate_pool.py          # 26 planted pool bugs, each must be caught
python3 engine/tests/mutate_registry.py      # 15 planted LifeRegistry bugs
python3 engine/tests/ghost_power.py          # the detector's false alarms and power
python3 actuarial/validate_fit.py            # annuity-factor errors from income start ages
(cd contracts && forge test)                 # fork tests hit live Robinhood Chain state
(cd sdk/ts && npm install && npm test)       # SDK against the real contracts on anvil
(cd web && pnpm install && pnpm dev)         # the site
python3 deploy/deploy.py <key-file> actuary|mortality|seal|core|handover|status
python3 services/keeper.py <key-file>        # settlement pages, rebalance, deaths, income, exits, reports
```

## Limits we know about

- **Income isn't guaranteed or inflation-linked.** It is paid in nominal USDG and follows the markets. For retirees of 2000, level-plan real income fell 51% by 2022, because T-bills paid 1.6% against the 3.5% the plan was priced at; the Escalating plan's fell 24%.
- **A research preview.** Deposits are capped at 25 USDG per member while it runs with real people.
- **The relaunch.** The core contracts are being redeployed after today's fixes; the site quotes the Actuary live on mainnet.
- **Identity is attested by the operator for now.** The built adapter checks an EIP-712 statement signed by the operator after a document check by video call. That is a trusted party, bounded by the timelocked, delayed allow-list. ZKPassport and national-ID QR adapters use the same interface without one.
- **The oracle check limits manipulation; it doesn't prevent it.** A trade can fill up to `maxSlippageBps` from a Chainlink price up to 26 hours old. A fresher price source is on the roadmap.
- **Settlement can stall on a paused token.** A rebalance can be abandoned, but income sales need their sleeve to trade.
- **Revival is capped by the reserve** (5% of every death's release, released at 1/60 a month). It covers members in full only while wrongful deaths stay rarer than about 1 in 20.
- **The ghost detector judges death rates against the UN tables.** It tests 85% of the tables' deaths against 55%. Measured over 3 years, it flags 1.6% of honest groups 20% healthier than the tables and 16% of those 30% healthier, more if deaths take long to become final, and the published rates cover 3 years of a lifelong pool. Concealment of less than about 40% of a group's deaths is left to the yearly identity renewal. A flag costs members an identity renewal, never money.
- **Recovery trusts the operator's identity check,** bounded by the 120-day delay the member can cancel. It can't win against someone holding the member's unlocked phone: they can cancel every recovery. They can't renew the member's identity, so income stops within 400 days, but the account can't be taken back from them on-chain.
- **Guardians keep a member from lapsing, but can't answer a death report.** A member who relies on guardians instead of checking in herself can be reported dead and not answer; the date of death is then her guardians' last confirmation.
- **Mortality past 90.** Gompertz misfits the plateau (Singapore women: survival error 0.054).
- **Regulation.** A longevity pool may count as insurance or a collective investment scheme. The path is the MAS FinTech Regulatory Sandbox, or a licensed trustee.

Data: UN World Population Prospects 2024 (CC BY 3.0 IGO). Market data: Chainlink on Robinhood Chain.
