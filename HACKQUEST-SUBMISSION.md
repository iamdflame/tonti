# Tonti: HackQuest submission (Arbitrum Open House Singapore Online Buildathon)

Every field of the HackQuest form, in its order, ready to paste. Character limits are checked by `video/data/check-submission.py`. Every claim is backed by the repo, the chain or a measured run, with the source given in `README.md` and `SUBMISSION.md`.

**Images to upload** are on the UNIQ drive in `/media/dflame/UNIQ/arbit/video/submission/`.

---

## Overview

### Name (5 / 80)
```
Tonti
```

### Intro (max 200)
```
Lifelong USDG income for migrant workers' parents. A Stylus actuary prices each pension on-chain from UN life tables in one call; savings earn in S&P 500 and T-bills on Robinhood Chain.
```

### Logo
Upload `00-logo.png` (1024×1024: the wordmark, its dot the lamp, on the dusk over the bay). If the logo shows very small, `00-logo-mark.png` is the mark alone (the lamp sun on the horizon).

### Sector (pick these 3)
- **DeFi**
- **RWA**: SPY and SGOV stock tokens on Robinhood Chain
- **Infra**: an on-chain actuary any protocol can call

### Tech Tag (8 / 8)
Select **Rust**, **Solidity**, **Next**, **React**, **Web3**, **Python** and **Node**, then *Add New* → **Arbitrum Stylus**.

### MVP Link
```
https://tonti-life.vercel.app
```

### Project Link
```
https://github.com/iamdflame/tonti
```

### X (Twitter) Link
Your own handle. Fill it in yourself: I don't know it.

### Wallet
Click **Connect Wallet** with the wallet you want rewards paid to. The form says it must use the same network as the hackathon (Arbitrum). Your member wallet or any Arbitrum wallet works.

### Images (4 / 4), 1280×720, in this order
1. `01-hero.png`: "$67.67 a month, for life", the chain's answer.
2. `02-live-quote.png`: "Income for life, priced on-chain", beside the live site's answer.
3. `03-architecture.png`: the six contracts and what flows between them.
4. `04-proof.png`: member #0's mainnet receipts, the first settlement, 59/59 bugs caught.

### Videos
- **Demo Video:** `https://youtu.be/z-4RYJNPF18`
- **Pitch Video:** optional. Leave it empty unless the form requires it; if it does, use the same link.

---

### Description (paste everything inside the block)

```
"How much would your mother get, every month, for the rest of her life?"

Tonti answers that question on Robinhood Chain in about a second, and then pays it, in USDG, for as long as she lives.

▶ Demo video (3 min): https://youtu.be/z-4RYJNPF18
▶ Live app, English and Filipino: https://tonti-life.vercel.app
▶ Code: https://github.com/iamdflame/tonti


THE PROBLEM

Singapore employs 1,635,700 foreign workers (Ministry of Manpower, Dec 2025). None of them can contribute to CPF, the city's pension. Most send money home to parents who have no pension either: only 24% of older people in South Asia receive one (ILO World Social Protection Report 2024–26). When the work stops, the money stops.

Saving alone doesn't fix it. Nobody knows how long they will live, so savers either spend too little or outlive their money. In our historical replay, money drawn alone ran out while 49% of the savers were still alive.


THE SOLUTION

Tonti is a pension pool with no pension fund: a fair, modern tontine whose rules are contracts.

1. Ask. Choose who it's for, their country, birth year, start age and savings. The Actuary contract simulates 512 possible lives and returns a monthly income range, in one eth_call, in about a second.
2. Join. Create a "life key": a passkey on your phone (Face ID or fingerprint). No seed phrase.
3. Pay in USDG, from your own wallet or from a child's wallet in Singapore.
4. It is invested along a glide path: S&P 500 (SPY) and US T-bill (SGOV) stock tokens via Uniswap v4, and USDG lent in Morpho.
5. From the chosen age, income arrives every month in USDG, for life. When a member dies, the savings they put at risk are shared fairly among those still alive (mortality credits). That is what lets the pool pay more than anyone can safely draw alone.
6. Every three months, one Face ID tap proves she is alive, verified on-chain.


WHAT IS LIVE ON ROBINHOOD CHAIN MAINNET (chain 4663)

• Six contracts, every one owned by a 48-hour timelock. The Solidity contracts are verified on Sourcify (exact match).
• A real member. Member #0 joined from an iPhone, checked in with Face ID (the passkey's P-256 signature verified on-chain), passed the identity check and paid in 25 USDG. The first monthly settlement invested it: $14.86 S&P 500, $7.06 T-bills and $3.04 USDG in Morpho, at 17 basis points of trading cost. The site shows the member's value live.
• The quote in the demo is a single eth_call. Anyone can re-run it on a public node and get the same number.


HOW TONTI MEETS THE JUDGING CRITERIA

Smart-contract quality
• Two Arbitrum Stylus (Rust) contracts do the heavy math on-chain. The Actuary does fixed-point mortality, the Monte Carlo quote and payout fractions. The TontiPool does cohort accounting, paged settlement, fair credits, exits and a ghost-member detector.
• A 512-path quote costs 9.5M gas. Our own Stylus ink meter predicted that before deployment, and mainnet matched it within 0.6%. The first design needed about 380M gas and could never have run.
• Settlement runs in pages that anyone can push. Paged and one-shot settlement end in byte-identical state, at about $0.03 per member-month.
• 68 Rust, 45 Solidity (10 against live mainnet state) and 6 TypeScript tests. Mutation testing caught 59 of 59 bugs we planted on purpose. Four independent adversarial reviews, with every finding fixed or disclosed.
• Governance is bounded in code. Mortality tables are sealed forever. Every parameter change waits 48 hours in public. Initialisation can't be front-run.

Product–market fit
• A named group with a monthly pain: Singapore's 1.6 million foreign workers and the parents they support. Citizens have CPF LIFE; nothing exists for them.
• Built for parents aged 55 to 80 on budget phones. Atkinson Hyperlegible type, English and Filipino, no seed phrases, a one-button Face ID check-in, and invite links a child can send over Messenger or WhatsApp.
• The first real member went through it on an iPhone and found six bugs our automated judge hadn't. All are fixed and now tested on every release.

Innovation
• An actuary on a blockchain: 1,846 cohort-specific mortality curves fitted to UN World Population Prospects 2024 (13 countries, both sexes, born 1935–2005; worst annuity-factor error 1.35%), sealed on-chain.
• A finite-pool correction we derived makes mortality credits fair even in small pools. At 100 members it cuts the bias from 4.8% to 0.26%.
• Proof of life without a trusted oracle: passkey check-ins, a 120-day death-report window answered by the member's own proof, revival for five years, and a statistical ghost-member detector (Wald's SPRT) inside every settlement.

Solving a real problem
• A historical replay, using UN mortality and real S&P 500 and T-bill returns: 2,000 Filipino women retire in 1965 with $10,000 each. Drawn alone, the money ran out in 1977 with 49% of them still alive. The pool paid every survivor, for life.
• By 1905 two-thirds of US life insurance was tontine insurance; New York banned it in 1906 after the insurers running it were found embezzling. Here the rules are contracts: no operator, not even us, can take the pot.

USDG
• Every dollar in and out is Paxos USDG: deposits, monthly income and death-report bonds. The cash sleeve earns in Morpho's USDG vault.


WHY ROBINHOOD CHAIN AND ARBITRUM STYLUS

• Robinhood Chain's SPY and SGOV stock tokens give a pension real equity and T-bill exposure, on-chain.
• Stylus makes actuarial math (exponentials, logarithms, Monte Carlo) cheap enough to run inside a single call.


TRY IT IN 60 SECONDS

1. Open https://tonti-life.vercel.app and ask about "Mother · Philippines · 1966 · income from 65".
2. Watch the answer arrive from the chain in about a second. Open "Run it yourself", then "Re-run it from this browser": a public node returns the same number.
3. Open "Live pool": members, holdings and the Actuary's odds by country, read live from mainnet.


HONEST LIMITS (research preview)

• Deposits are capped at 25 USDG per member.
• Identity is attested by the operator for now; ZKPassport and national-ID adapters are next.
• A longevity pool may count as insurance in Singapore: the path is the MAS FinTech Regulatory Sandbox or a licensed trustee.
• Not financial advice. Not an insurance product.


ROADMAP

1. Gasless onboarding for parents: Alchemy EIP-7702 smart wallets with sponsored gas.
2. Trust-minimised identity: ZKPassport and national-ID QR adapters.
3. The first 100 members, from Singapore's Filipino domestic-worker community. Caps rise as pools grow.
4. The MAS sandbox or a licensed trustee. More countries: each one is a new sealed Actuary.
```

---

### Progress During Hackathon (paste)

```
Shipped during the buildathon, from first mainnet probe to a real member, dated:

• Sep 26. Recon on Robinhood Chain mainnet (token addresses, Uniswap v4 liquidity, Chainlink feeds). We built a Stylus ink meter that prices our WASM exactly as Stylus meters it, so gas was known before deploying. Activation dry-runs passed on mainnet.
• Sep 27. The first Actuary went live on mainnet: a 512-path quote in one eth_call, matching the meter's prediction within 0.6%.
• Sep 27. The same day, an independent adversarial review found two fund-level bugs: a top-up could take other members' income, and anyone could "kill" a living member for 10 USDG. Both were fixed with regression tests, along with 13 further findings. We added mutation testing to prove the tests can fail.
• Sep 27–28. We built the site, "Lifeline at dusk": Next.js, English and Filipino, passkeys, the live quote and "Run it yourself". A judge script runs against the public URL: 20/20 quotes equal a direct eth_call, 80/80 routes load clean, 0 accessibility violations.
• Sep 27–28. Reviews 2 to 4 hardened revival, estates, account recovery, guardians and presumption of death. Planted bugs: 59, caught: 59.
• Sep 28. Relaunch on mainnet: a sealed Actuary, TontiPool, Treasury, LifeRegistry and AttestedIdentity, with every contract handed to a 48-hour timelock. Solidity verified on Sourcify.
• Sep 30. Added Ghana, the first member's country. Actuary v3 holds 1,846 cohorts; every cohort was read back from its on-chain events and matched before sealing. New core and timelock.
• Oct 1. Member #0 joined from an iPhone, checked in with Face ID (verified on-chain), passed the identity check and paid in 25 USDG. The first monthly settlement invested it in SPY, SGOV and Morpho USDG. The attempt exposed six UX bugs, among them MetaMask's in-app browser refusing passkeys and an empty embedded wallet chosen over MetaMask. All six were fixed and added to the public-URL judge.
• Oct 2–3. The demo film was recorded from the live site and mainnet, one continuous take of the quote with the chain's real 1.2-second answer. Every number on screen is read from the chain.

Totals: 6 contracts on mainnet, 119 automated tests, 59/59 planted bugs caught, 4 adversarial reviews, 13 countries priced, 1 real member invested, 97 commits.
```

---

### Fundraising Status (paste)

```
Not raised; self-funded so far. The team paid the mainnet gas and made the first real deposit.

What funding would unlock next:
• An external audit of the Stylus contracts (Actuary and TontiPool) before deposit caps rise.
• Gas sponsorship so parents can join and check in without holding ETH (Alchemy EIP-7702).
• A MAS FinTech Regulatory Sandbox application, or a partnership with a licensed trustee.
• Community onboarding with Singapore's Filipino domestic-worker groups for the first 100 members.

Open to Arbitrum and Robinhood Chain ecosystem grants and to accelerator conversations.
```

---

## Deployment Details (visible only to the judges)

- **Ecosystem deployed:** **Robinhood Chain** if it's listed (it runs Arbitrum technology, and our Rust contracts are Arbitrum Stylus). If it isn't listed, choose **Arbitrum**.
- **Testnet / Mainnet:** **Mainnet**

### Contract address and deployed link (paste)
```
Robinhood Chain mainnet (chain 4663). Live app: https://tonti-life.vercel.app

Actuary (Arbitrum Stylus, Rust; mortality sealed): 0x1464069e9f1110e6f0f52231a2c1f87c2fc9ccaa
https://robinhoodchain.blockscout.com/address/0x1464069e9f1110e6f0f52231a2c1f87c2fc9ccaa

TontiPool (Arbitrum Stylus, Rust): 0xfde46333804f2ea5167bdb7f8f7408ab0cee6308
https://robinhoodchain.blockscout.com/address/0xfde46333804f2ea5167bdb7f8f7408ab0cee6308

Treasury (Solidity, Sourcify exact match): 0xb1273Eda4380039CaaBDAb79b1Ad756EEf1412Bc
https://robinhoodchain.blockscout.com/address/0xb1273Eda4380039CaaBDAb79b1Ad756EEf1412Bc

LifeRegistry (Solidity, Sourcify exact match): 0x97e1D2f4E7d0B86aa85dc6691EA5cBF7175fC03D
https://robinhoodchain.blockscout.com/address/0x97e1D2f4E7d0B86aa85dc6691EA5cBF7175fC03D

AttestedIdentity (Solidity, Sourcify exact match): 0xd715390236b1f3c43689D74ab61b34B09E5aa1C7
https://robinhoodchain.blockscout.com/address/0xd715390236b1f3c43689D74ab61b34B09E5aa1C7

TimelockController, 48 h (owns all of the above): 0x5986346B942D30C8Bdc9CB75a4F230140Ce5C504
https://robinhoodchain.blockscout.com/address/0x5986346B942D30C8Bdc9CB75a4F230140Ce5C504

Deployed from block 76,631,644. Real usage, member #0:
Join: https://robinhoodchain.blockscout.com/tx/0xb5ddd1965943c8dea83c8069c7853d3c8ae66dd21b520f6f84f7cb073a2e5a40
Face ID check-in: https://robinhoodchain.blockscout.com/tx/0x263439747406fc0d7094da44161f4adba5ead0fe20c0529546e75f8f045dd739
25 USDG deposit: https://robinhoodchain.blockscout.com/tx/0x26861ca62fe1c8a901905628f5bb915a0f30480496a4668586bea1c482be7b86
First settlement (invested): https://robinhoodchain.blockscout.com/tx/0x97a8a6c10b18895bdffa1399070b100ed8ca9aa7af21072ca878118a42621bf2
```

---

## Checkpoints (one entry each: Type · Title (max 50) · Description (max 200) · Link · Image)

| # | Type | Title | Description | Link | Image |
|---|---|---|---|---|---|
| 1 | Idea | CPF LIFE for the workers CPF leaves out | Singapore's 1,635,700 foreign workers can't join CPF, and their parents have no pension. Tonti: a fair tontine on Robinhood Chain paying USDG income for life. | https://github.com/iamdflame/tonti | `01-hero.png` |
| 2 | Development | Actuary in Stylus: 512 lives in one eth_call | Rust fixed-point mortality and a Monte Carlo quote. Our ink meter predicted 9.5M gas before deploying; mainnet matched within 0.6%. The first design needed 380M. | https://tonti-life.vercel.app | `cp-quote.png` |
| 3 | Testing | 4 adversarial reviews, 59/59 planted bugs caught | Day one, a reviewer found 2 fund-level bugs; both fixed with regression tests. Mutation testing proves the tests can fail: 36/36 in the pool, 23/23 in the registry. | https://github.com/iamdflame/tonti | `cp-safeguards.png` |
| 4 | Launch | Six contracts live on Robinhood Chain mainnet | Sealed Actuary (1,846 UN cohorts), TontiPool, Treasury, LifeRegistry and AttestedIdentity, all owned by a 48 h timelock. Solidity verified on Sourcify. | https://robinhoodchain.blockscout.com/address/0xfde46333804f2ea5167bdb7f8f7408ab0cee6308 | `03-architecture.png` |
| 5 | Design | Built for parents, in English and Filipino | Face ID instead of seed phrases, a one-tap check-in, hyperlegible type. Judged on the public URL: 20/20 quotes match the chain, 80/80 routes clean, 0 a11y issues. | https://tonti-life.vercel.app/fil | `cp-filipino.png` |
| 6 | Launch | First real member, first settlement | Member #0 joined from an iPhone, checked in with Face ID (verified on-chain) and paid in 25 USDG. The settlement invested it: $14.86 SPY, $7.06 SGOV, $3.04 Morpho. | https://robinhoodchain.blockscout.com/tx/0x97a8a6c10b18895bdffa1399070b100ed8ca9aa7af21072ca878118a42621bf2 | `04-proof.png` |
| 7 | Testing | History replay: 1965 with real markets | 2,000 Filipino women retire in 1965 with $10,000 each, with UN mortality and real S&P and T-bill returns. Alone, the money ran out in 1977 with 49% alive; the pool paid all. | https://tonti-life.vercel.app | `cp-replay.png` |
| 8 | Other | Demo film recorded from the live chain | 3 min: one continuous take of the live quote (1.2 s from tap to answer), mainnet receipts, the Filipino version. Every number on screen is read from the chain. | https://youtu.be/z-4RYJNPF18 | `cp-member.png` |

---

## Before you press submit

- [ ] Logo and the four images uploaded, in order.
- [ ] Demo video link plays (it's public: "Tonti: Income for Life, Priced On-Chain").
- [ ] X link and wallet are yours.
- [ ] Deployment details filled in (judges only).
- [ ] All 8 checkpoints added.
- [ ] Repo is public: https://github.com/iamdflame/tonti
