# Tonti protocol specification (v0.2, 2026-09-27)

A lifelong-income pool on Robinhood Chain. Members pay in USDG and are paid USDG every month for as long as they live. Money released by members who die is shared among the survivors as *mortality credits*. There's no insurer and no guaranteed rate: income adjusts to realized returns and realized deaths, so the pool cannot become insolvent.

Notation: epochs are months `t = 0, 1, 2, …`. Money is valued in USDG.

## 1. Cohorts

A **cohort** `k` is a tuple `(birthYear, sex, country, startAge, plan, class)`. The plan is level or escalating (§4.1). The class is either:
- `T` (tontine): at risk, earns credits
- `B` (bequest): protected, earns no credits, **pays no income** (it stays invested), and goes to beneficiaries at death. Paid out at a life-contingent rate without credits it would dwindle, so a level plan wouldn't stay level; only `T` cohorts pay (skeptic review 2)

Everyone in a cohort has the same attained age, glide path and payout rate. That's what makes every member-level quantity proportional to a cohort-level quantity, and what makes settlement O(number of cohorts) instead of O(number of members).

Per cohort, the protocol stores:
- `s[k][a]`: shares of each asset sleeve `a ∈ {cash (Morpho steakUSDG), SGOV, SPY}`
- `U[k]`: total units held by living members
- `I[k]`: cumulative income per unit, in USDG (lazy payouts; see §5)

The cohort's value is `value_k = Σ_a s[k][a]·P_a`, and its unit value is `V_k = value_k / U[k]`.

A member holds `u_i^T` units of their `T` cohort and `u_i^B` units of the matching `B` cohort. The bequest share `β_i = u_i^B·V_B / (u_i^T·V_T + u_i^B·V_B)` is chosen at join time and can only be lowered.

## 2. Mortality model

For each `(country, sex, birth year)` cohort, a Gompertz–Makeham force of mortality fitted along that cohort's own diagonal of the UN tables:

```
μ_b(x) = A_b + B_b · exp(θ_b·x)
```

A first version used one model per `(country, sex)` with a single calendar-time improvement rate, `μ(x, y) = A + B·exp(θx − κ(y − 2024))`. It measured up to 9.8% annuity-factor error in fast-improving countries, because improvement varies by age. Per-cohort fitting absorbs each cohort's own improvement path. The engine keeps the κ term, set to 0 for cohort parameters.

Here `x` is age in years, `y` is the calendar year, and `κ` is the rate of mortality improvement. The survival probability from age `x` in year `y` over `τ` years has a closed form, because age and calendar time advance together:

```
S(x, y, τ) = exp( −A·τ − B·e^{θx − κ(y−2024)} · (e^{(θ−κ)τ} − 1)/(θ − κ) )
```

- **Fit.** From UN WPP 2024 single-age life tables, along each cohort's diagonal (estimates through 2023, medium projections after; key `(isoNumeric·2 + sex)·10000 + birthYear`):
  - a least-squares fit on `ln m_x` from the cohort's age in 2026 (at least 30) to 99 gives the starting point;
  - A, B, θ are then refined to match WPP's annuity factor ä (3.5%) from every age income can start (50, 55, …, 80, and the cohort's own age if older) and, at half weight, from 85, 90 and 95, since income is paid at `1/ä` from every age reached. The `ln m_x` residuals are kept as a light regulariser (weight 0.05) so saving-phase death rates stay sensible.
- **Measured** (`actuarial/validate_fit.py`) against the same UN tables, so this is fit error, not out-of-sample validation; cohorts born after 2000 can't be checked to age 100 against projections that end in 2100:

  | Annuity factor from | Worst error | 95th percentile | Median |
  |---|---|---|---|
  | the start ages (50–80, or the cohort's own age), 7,680 pairs | **1.35%** | 0.60% | 0.16% |
  | 85 | 1.30% | 0.77% | 0.28% |
  | 90 | 1.70% | 1.10% | 0.25% |
  | 95 | 3.74% | 2.07% | 0.46% |

  - History: the first version fitted `ln m_x` only and was checked from each cohort's age in 2026; it reported 0.66% but was up to 3.09% from the start ages (review 1). The second targeted the start ages only: 1.13% there, but 2.2%, 4.7% and 7.3% from 85, 90 and 95 (review 2). Weighting the later ages cost 0.2 points at the start ages and halved the error beyond.
  - survival errors are largest for Singapore women past 90 (0.054), where Gompertz misfits the mortality plateau.
- **Sealed.** Once every country is loaded, `Actuary.seal()` makes the tables permanent: no owner, the timelock included, can change them. New tables mean a new Actuary and a new pool.
- **The 1-month death probability** used in settlement is `q_k(t) = 1 − S(x_k(t), y(t), 1/12)`.

## 3. Mortality credits (fair allocation)

At each settlement, `D` is the set of members whose death became final in the epoch (§6).

1. **Release.** For each dead member `j` in cohort `k`:
   - their tontine units leave the cohort, and cohort `k` gives up the matching slice of its sleeve shares, `s[k][a]·u_j^T/U[k]`, into a release bucket `rel[a]`
   - `U[k] −= u_j^T`
   - their bequest units become a claim for their beneficiaries (§7)

   The released value is `R = Σ_a rel[a]·P_a`.
2. **Weights.** Each surviving tontine cohort gets weight `w_k = h_k·value_k`, where `h_k = q_k/(1 − q_k)` are the one-month odds. Let `W = Σ_k w_k`, `s_k = w_k/W`, and the concentration `Σs² = Σ_k s_k²`. The weight actually used is corrected for the pool's size: `y_k = w_k·(1 + s_k − Σs²)`. That gives `Σ_k y_k = W`, so credit moves between members and none is created. It's rounded so that `Σ y ≤ W` holds exactly.
3. **Transfer in kind.** Each surviving cohort `k` receives `rel[a]·y_k/W` shares of every sleeve. No trade happens: the pool's total holdings don't change, and the shares simply change owner. Allocation drift is fixed later by the normal rebalance (§8).

Each survivor's wealth therefore grows by the factor `1 + ρ·h_k`, where `ρ = R/W`.

**Why this is fair.** A member's expected gain over an epoch is `(1 − q)·E[credit | survive] − q·(at-risk wealth)`. With plain weights `w`, `E[credit | survive] = q·T/(1 − q)` only in a large pool. The survivors' expected weight equals the expected release, but two finite-pool effects remain.
- **The member's own weight.** It sits in the denominator of their own credit, costing them their share `s_i`.
- **The variance of deaths.** Release and survivor weight move against each other, which pays everyone `Σs²`.

So the relative bias is `−s_i + Σ_{j≠i} s_j²` to first order. That's `O(1/N)`, and it falls on the members with the biggest weight shares: old and rich. The correction `y = w·(1 + s − Σs²)` cancels it, leaving about `−s_i²`. It costs one extra 256-bit accumulator (`Σw²`) in the weighing pass.

**Measured** (`actuary-cli fairness`, the real ledger, members aged 40–90 with $1k–$30k each, one cohort each; every survivor's credit reproduced exactly from the formula). Both rules are shown on the same members and the same deaths:

| members | trials | plain `h·value` weights: worst / RMS bias | corrected: worst / RMS bias | ledger checks |
|---|---|---|---|---|
| 20 | 60,000 | +34% / 31% | +10.5% / 4.9% | 1.19M |
| 100 | 20,000 | −6.8% / 4.8% | **−1.6% (SE 1.4%) / 0.26%** | 1.99M |
| 1,000 | 20,000 | −0.98% / 0.64% | +0.30% (SE 0.50%) / 0.18% | 19.9M |
| 10,000 | 20,000 | −0.13% / 0.051% | +0.03% (SE 0.15%) / 0.012% | 199M |

From 100 members up, what remains is indistinguishable from Monte Carlo noise (mean |z| 0.11 at 100). Pools of a few dozen still carry a few percent. The next order of the same expansion leaves `2s_iV − V²` (V = Σs²), which suggests `y = w·(1 + s − V − 2sV + V²)` (Σy = W·(1 − V²) ≤ W). A float prototype was too noisy to confirm it, and it needs the exact simulator at 20k+ trials before it's built. Below about 50 members, Sabin's exact fair transfer plan (2010) is the complete answer; it is designed, not built.

**What else follows:**
- **Mortality-level errors cancel.** If the pool's true mortality is uniformly lower than modelled (annuitants tend to be healthier), `ρ` falls by the same factor for everyone and fairness holds. Only *relative* errors between cohorts create unfairness. The simulator measures that too.
- **No insolvency.** `ρ` redistributes only what was released: `Σ credits = R` exactly (an invariant).

## 4. Payout rule (natural tontine)

A paying cohort at age `x` pays out the fraction `p_k(t) = 1 / ä_k(t)` of its value each month, where

```
ä_k(t) = Σ_{m=0}^{12·(120−x)} S(x, y, m/12) · (1 + r)^{−m/12}
```

`r` is the valuation rate, set by governance within `[2%, 5%]`, launch value 3.5%. It is the one governance lever over payouts: across the band, payout fractions move by about ±15%. If returns equal `r` and deaths match the model, each survivor's expected income stays level for life. When returns or deaths differ, income moves in proportion. That's the honest trade for having no insurer's capital and no insurer's cut.

Month to month, a survivor's income changes by about `(1 + R_t)/(1 + r)^{1/12}`, where `R_t` is the cohort's realized return net of fees, provided deaths match the model. Income rises when the pool earns more than `r` and falls when it earns less.

### 4.1 Plans: level and escalating

Nominal income that stays level loses purchasing power. CPF LIFE answers this with an Escalating Plan that starts lower and rises 2% a year. The natural-tontine equivalent needs no new machinery: price the cohort at a lower rate.

- **Level plan:** `p_k = 1/ä_k` at `r`.
- **Escalating plan:** `p_k = 1/ä_k` at `r − e`. The escalation `e` is set by governance within `[0%, 4%]`, launch value 2%. The plan pays out less today and keeps more invested, so when returns equal `r`, income grows by about `e` a year.

The plan is chosen at join and is part of the cohort's identity, so level and escalating members never share a payout rate. Mortality credits still flow across all cohorts by `h·value` (§3), because the plan changes when money is paid out, not who is at risk of dying.

Measured (`actuarial/personas.py`, same engine as the contract): Maria starts at $129 a month instead of $162, and at 85 her median income is $247 instead of $189. For her mother, the chance of outliving the same starting income drawn alone falls from 49% to 24%. In the historical replay (§10), the escalating plan's worst real year beats the level plan's in every cohort tested.

### 4.2 The on-chain quote

`quote()` runs a Monte Carlo of monthly market returns for the saving and paying years, with expected mortality credits, and reports income at the start and at 85 (P10/P50/P90). It also reports the same starting income drawn alone from the same pot: the median age it runs out, and the chance of being alive then. It's deterministic (the seed is a hash of the inputs) and reproducible off-chain by `actuary-cli quote`. An independent float model (`engine/tests/reference_quote.py`) agrees with it to 1.1e-11 over 40 random quotes, with random markets, rates, fees and path counts. The pool's income in the quote is net of its 0.30% yearly fee, taken monthly; the saved-alone comparison pays no pool fee.

**Inputs are bounded** to what the pool can take: age 18 or more, income from 50 to 80, no older than the start age, the years 2020–2100, at most 10M USDG paid in now and 100k USDG a month. Anything else is refused (`BadInput`) rather than extrapolated. An independent review had found a quote of 8.48e15 USDG a month for an 86-year-old starting at 119.

**The return model.** SPY's monthly growth is lognormal with the disclosed μ and σ. It is sampled from a 256-point table built at equiprobable normal quantiles, rescaled to exactly unit variance, and scaled so its mean equals the lognormal's mean. Over the hundreds of monthly draws in each path the discreteness vanishes. Everything that doesn't depend on the path is computed once per quote: the survival and payout schedule, glide-path weights, `k_m = (1 − f_m)/S_m`, and cumulative survival. A path-month then costs a table lookup and four multiplies.

**Measured cost** (`engine/ink-meter`, Stylus v3 pricing from nitro, on the deployable WASM):

| paths | 16 | 64 | 128 | 256 | 512 |
|---|---|---|---|---|---|
| gas | 0.85M | 1.69M | 2.79M | 5.01M | 9.46M |

`quote()` is capped at 512 paths, which uses 30% of the 32M per-call cap. The first version drew each month with an inverse normal and an exponential and needed ~48M gas at 64 paths. It could never have run.

## 5. Lazy per-member accounting

- **Settlement.** Every paying cohort sells the fraction `p_k` of its sleeve shares for USDG into the payout vault, and does `I[k] += V_k·p_k`.
- **Claims.** A member claims `u_i·(I[k] − I_i^last)` USDG. That's O(1), and nothing loops over members.
- **Contributions** mint units at the current `V_k`, after that epoch's settlement. The USDG goes to the cash sleeve. The payer can differ from the annuitant: a daughter can fund her mother's account. The minimum is 1 USDG, so every queued deposit costs its sender real capital. **Contributions open only once the member's identity is verified** (§6). Income and exits need a verified identity, so money paid in before verification could never come out.

### 5.1 Paged settlement

A settlement touches every cohort, every queued death and every queued deposit. Done in one transaction, its gas grows with the pool until it no longer fits in a block, and income stops for everyone. `join` and `contribute` are permissionless, so an attacker could force that by opening many small accounts across many cohorts.

So settlement runs in pages. `settleSteps(budget)` does at most `budget` items (one death, cohort or deposit each) and stores where it stopped. Anyone can push the next page, so no keeper can stall it.

1. **Begin:** check the epoch has elapsed; freeze the prices, the time and the fee; fix the queues' end points.
2. **Release** each queued death or exit (§6, §7).
3. **Weigh** each cohort: charge its fee if a release hasn't already, and record its credit weight `h·value` and payout fraction from the Actuary.
4. **Credit** each cohort `w_k / Σw` of the released shares, in kind, and take its income sale.
5. **Sell** once: income, bequests, exits and fees.
6. **Pay** each released member's beneficiary, or the member for an exit.
7. **Book** each paying cohort's income per unit, and record every cohort's income index for this epoch.
8. **Invest** waiting deposits in one deposit, then **mint** each member's units at the frozen NAV.

**Every step reads other cohorts only through a running total,** so the order of pages can't change the result. `actuary-core` implements the steps once (`ledger::step`, `rebalance::step`), and the one-shot `Epoch::settle` is the same steps run over every cohort. Measured:
- the paged order equals the one-shot ledger **exactly**, share for share and unit for unit, over 10,000 random settlements
- in the pool, a busy month (a death, a bequest, two plans paying, a deposit) settled one item per transaction ends in **byte-identical** state to settling it in one. So does the following rebalance.

**Queue semantics.**
- The queues are append-only: a settlement consumes `[head, end)` as it stood when the settlement began.
- Deaths and exits queued mid-settlement wait for the next epoch.
- Deposits and parameter changes are refused until the settlement finishes. Claims are allowed throughout.
- A rebalance can't start during a settlement, and a settlement can't start during a rebalance.

**Income history** (for estates, paid up to the date of death) is found by binary search over epoch timestamps, O(log epochs).

## 6. Proof of life

The pool must never pay the dead, and must never be able to kill the living. Member states and transitions:

```
ACTIVE ──(check-in period 90d passes)──▶ DUE ──(grace 30d)──▶ LAPSED ──(24 months)──▶ PRESUMED_DECEASED
  ▲                                       │                     │                              │
  └────────── passkey check-in ───────────┘                     │                              │
  └────────── strong proof (identity-bound) ────────────────────┘                              │
any state ──(bonded death report)──▶ DEATH_REPORTED ──(120d, no proof of life)──▶ DECEASED      │
                                         └──(member proves life)──▶ ACTIVE; bond → member      │
DECEASED or PRESUMED_DECEASED ──(identity proof within 5 years: revive)──▶ ACTIVE; repaid ◀────┘
```

- **Who can be paid.** Income and exits go only to a member who meets all three conditions (`canReceiveIncome`):
  1. **alive:** ACTIVE, or DUE within grace
  2. **identified:** an allow-listed identity adapter has confirmed the member holds the identity of the cohort they were priced in (birth year, sex, country)
  3. **recently strong:** an identity statement issued within `strongPeriod`, 400 days (governance bounds 180–730)

  Income accrues in every state and is held, not lost, until all three hold.
- **Why identity is bound to the cohort key.** In a tontine, claiming to be older than you are is the classic fraud: the pool pays you credits priced for a higher death risk than you carry. Joining records the cohort key in the LifeRegistry but proves nothing, so money goes in and leaves only after an adapter attests that key. Guardians are the member's own choice, so they can vouch that someone is alive but never for identity or age.
- **Death reports can't kill the living.** Anyone may report a death, with a 10 USDG bond. The member answers it with any proof of life after the report was filed, and the bond then goes to them.
  - The **challenge window is 120 days**, and governance can't set it shorter than the check-in period plus grace. A member who simply keeps their 90-day check-ins answers any report without ever knowing of it. An independent review showed the old 30-day window let anyone permanently kill an active member for a 10 USDG bond.
  - A report that goes unanswered becomes final, but the reporter's bond is **held for a year**. If the member proves they're alive in that year, the bond goes to them; otherwise it goes back to the reporter (`releaseBond`).
  - There is **no reward for reporting**. An earlier bounty (six months of the member's income) made a false report profitable. The beneficiary's estate is reason enough to report a real death, and the identity proof every 400 days catches a hidden one.
  - **A reported death's estate is held for a year** (`ESTATE_HOLD`): the income owed up to the date of death, any uninvested contribution and the bequest wait in `m_estate`, and `pay_estate` pays them to the beneficiary once a year has passed and the registry still says the member is dead. The income dated after the death waits too (`m_post`) and then goes to the survivors; every reported release restarts the hold, and a revived member gets all of it back with her restore (skeptic review 4: a false report otherwise took ~4–7 months of her income for good). If the member is revived in that year, `pay_estate` invests them back into the member's account instead. Otherwise a beneficiary could report a parent who couldn't answer in time and collect the bequest early (skeptic review 3). Presumed deaths, after two years of silence, pay the estate at once.
- **Any death can be undone within five years.** A member whose death became final, whether reported or presumed, proves their identity through an allow-listed adapter bound to their cohort key (`LifeRegistry.revive`). The statement must be issued after the death became final.
  - They are then repaid their released at-risk shares from the revival reserve, as far as it goes (`TontiPool.restore`, which accepts any death release, never an exit, and only while the registry says the member is alive). The shares are sold with the month's sale and **invested back into their own account** at the next settlement, never paid out: repaid in cash, a staged death was a way to leave after income had started (skeptic review 2). What the reserve can't cover stays owed (`m_owed`, flag `OWED`), and `restore` can be called again as the reserve refills; in a young pool the first repayment can be as little as the 5% the member's own release put in (skeptic review 3 measured 3.9%). A later release adds to what is owed, never replaces it. A restore that meets the member's real death in the same settlement leaves her released and puts the repayment in her estate; a member who has exited is repaid in cash; a repayment for a member already queued to deposit that month waits in `m_restored` and is invested the month after, so nothing is minted from money not yet invested (skeptic review 4).
  - A member revived after being queued dead but before the settlement reaches them is not released at all: the release re-reads the registry (skeptic review 3: she was released anyway, losing her unclaimed income).
  - Every final death clears any earlier revival (`revivedAt = 0`), so a revival left over from an earlier false death can't repay a real one.
  - The **revival reserve** takes 5% of every death's at-risk release, reported or presumed, and flows on to the survivors at 1/60 a month (a mean holding of five years).
  - The bequest share already paid to their beneficiary isn't clawed back.
  - Tested: a false report that went unanswered is revived; the member is repaid from the reserve at the sale price into their account, holds units again the month after, and the reporter's held bond goes to them. A statement from before the death became final is refused. A death older than five years stays final. A stale revival while the registry says the member is dead repays nothing.
- **Date of death.** When a death becomes final, income for epochs after the date of death returns to the pool as released money. Income earned before it is the member's estate, and goes to the beneficiary. A mutation that pays the estate to the payout address instead fails the tests.
- **Presumption when identity proofs lapse.** Heirs holding a dead member's passkey can keep checking in, but they can't prove the dead member's identity. So beside the usual rule (24 months with no proof of any kind), an identified member may be presumed dead after two strong periods (800 days) without an identity-bound proof, dated at the last one.
- **Proofs:**
  - **check-in:** a passkey signature (P-256 via RIP-7212) over a per-member nonce; resets liveness.
  - **guardians:** 2 of the member's 3 guardians within 30 days; resets liveness only. Guardians can't answer a death report: only the member's own passkey or an identity statement does (`lastOwnProof`). A report's date of death is moved up to the member's last proof of life, guardians' included, if it is earlier. The three guardians must be different addresses. A presumption closes any report still open and returns its bond, so the report can't rewrite the death afterwards.
  - **strong:** an identity adapter's statement, `IIdentityVerifier.verify(memberId, key, action, proof) → issuedAt`. It identifies the member the first time. The registry accepts a statement only if it was issued after the last accepted one, and records the issue date. So a statement can't be replayed to stretch the 400 days, nor one from before a ghost-detector flag used to clear it.
    - Built: `AttestedIdentity`. It takes EIP-712 statements `Identity(memberId, key, registry, action, issuedAt, expiry)` from an attester key, each valid at most 30 days.
    - In the research preview the attester is the operator, after a document check by video call. That is a trusted party, and it's disclosed. ZKPassport and signed national-ID QR codes (PhilSys, Aadhaar) implement the same interface without one.
  - **Adapters join slowly and leave at once.** A newly allowed adapter is usable only 30 days later (`VERIFIER_DELAY`); removal is immediate. During set-up, before the pool is connected, nobody is enrolled, so the first adapter works at once.
- **Accounts can change.** The payout address can change the payout address, the beneficiary and the guardians (`setPayout`, `setBeneficiary`, `setGuardians`).
  - A member who lost their phone or wallet recovers with an identity statement whose `action` is bound to the new payout address and passkey (`recover`). It takes effect after the **challenge window** (120 days, never less than a check-in period plus grace; `finishRecovery`, anyone may call), and a check-in with the member's current passkey, or her payout address (`cancelRecovery`), cancels it; no presumption for a lapsed identity while one is pending. A member on her usual schedule therefore always cancels a recovery she didn't ask for; with the first design's 14 days, one requested the day after her check-in completed before her next one (skeptic review 3). Income waits while a recovery is pending. A request is not an identity proof until applied (`lastRecoveryIssued` tracks its statements apart from `lastStrong`), so the victim of a stolen phone can't reopen income to the thief by trying to recover. Once applied, any check-in signed by the old passkey becomes void.
  - **Limit:** whoever holds the member's unlocked phone can cancel every recovery. They can't renew the identity, so income stops within 400 days, but the account can't be recovered from them on-chain.
  - The pool reads payout and beneficiary from the registry, so there is one source of truth.
  - The bequest share can't change after joining.
- **The ghost detector's baseline (a decision, not a gap).** It judges deaths against the fitted UN mortality, testing λ = 0.85 against λ = 0.55. The first version tested 1.0 against 0.7, whose drift turns positive for any group below 0.84 of the tables, so every honest group that much healthier (healthy-annuitant selection) would eventually be flagged. The new test's drift turns positive only below λ ≈ 0.69.
  - **Why not learn λ̂ from each group's own experience:** that would make day-one concealment invisible, because a group hiding 30% of its deaths from the start would converge to a lower λ̂. No test on death rates alone can tell healthy from hiding without an outside baseline; the UN table is that baseline.
  - **What a flag costs:** an identity renewal within 120 days, never money.
- **Ghost-member detector** (on-chain, inside settlement). Per group `g` (country × birth decade: the same people as a 10-year age band, but nobody ages out of a flagged group), it runs Wald's SPRT on deaths, Poisson with mean `λ·E_g`, where `E_g = Σ q_i` over the group's members on the books:
  - **Reporting lag.** Honest deaths become final 4–6 months after they happen (a report, then the 120-day window). So each epoch's *final* deaths are tested against the deaths expected `lag` epochs earlier: `lag = ⌈150 days / epoch length⌉`, kept in an 8-epoch ring per group.
    - Testing each month against itself, as the first design did, flagged **100%** of honest groups by month 3 in a simulation with realistic lags.
    - The pool had already been built that way; the independent review's hint about reporting lag led to this.
  - H0 `λ = 0.85`, H1 `λ = 0.55` (deaths being hidden). `LLR += d·ln(0.55/0.85) + 0.3·E`, with **α = 0.1%, β = 5%**. The threshold `ln(0.95/0.001)` is a Q64.64 constant.
  - Epochs are bounded to 28–31 days, so the 150-day lag (5 epochs) always fits the 7 usable slots of the ring. With 1-day epochs, allowed before, the lag was silently clamped to 7 days and the first design's failure returned.
  - Crossing it flags the group (`GroupFlagged`). Its members then have **120 days** to give an identity statement issued after the flag; after that, their income and exit are held until they do.
  - Tested in the pool: 10 members, 9.9 expected deaths a month and none final, are not tested until the lag has passed and are flagged in the third tested month. A claim still works at day 119 after the flag and is refused at day 120 until a fresh statement. Mutations removing the lag or the grace each fail the test.
  - A mirror test with H1 `λ = 1.5` catches mass false death reports. It emits `GroupFlagged` kind 2 for review.
  - **Measured** (`engine/tests/ghost_power.py`; 3,300 Philippine women born 1956–1966; 10 seeds × 300 runs × 36 months; honest deaths final after 4–6 months unless stated):

    | Deaths hidden | First design (H1 0.7, α 1%, no lag) | H1 0.7, α 0.1%, lag 5 | **As built (H0 0.85, H1 0.55, α 0.1%, lag 5)** |
    |---|---|---|---|
    | 0% (honest) | 100% flagged by month 3 | 0.13% within 24 months | **0%** within 36 months |
    | 10% | — | 0.9% (24 m), 3.7% (36 m) | 0% (24 m), 0.03% (36 m) |
    | 20% | — | 10% (24 m), 32% (36 m) | 0.4% (24 m), 1.2% (36 m) |
    | 30% | — | 47% (24 m), 86% (36 m) | 6.2% (24 m), 17% (36 m) |
    | 40% | — | — | 37% (24 m), 74% (36 m), median 28 months |
    | 50% | — | 99.6% (24 m) | 83% (24 m), 99.3% (36 m), median 19 months |

    Honest groups (nothing hidden), flagged within 36 months, by how healthy they are and how long their deaths take to become final (5 seeds × 300 runs each):

    | True mortality vs the tables | final in 4–6 months | 5–8 | 6–9 | 4–12 |
    |---|---|---|---|---|
    | 100% | 0% | 0% | 0.4% | 2.3% |
    | 90% | 0% | 0.2% | 2.7% | 6.9% |
    | 85% | 0.4% | 1.2% | 5.3% | 12.9% |
    | 80% | 1.6% | 4.2% | 13.7% | 23.7% |
    | 70% | 16% | 33% | 52% | 68% |

    The old test flagged 5.2%, 11.5%, 30% and 84% of honest groups at 90%, 85%, 80% and 70% of the tables within 36 months (skeptic review 2). Long reporting delays still cost false alarms, almost all in a group's first months. A flag costs its members an identity renewal within 120 days, never money.

    The detector is the backstop against *organized* concealment. Individual concealment is stopped by the identity statement every 400 days: hiding a death means faking an identity-bound proof of a dead person.

## 7. Bequests, exits and fees

- **Bequest claims:** a dead member's `B` units are redeemed for their beneficiaries at the next settlement.
- **Exit:** only during accumulation, after a **12-month notice** (governance range 180–730 days). Notice is the anti-selection defense: someone who learns they're ill can't walk away with money the pool priced as at risk.
  - **Giving notice.** Only the member's own payout address can give notice, and only if the notice ends before income would start. The member can cancel until the exit is queued. No new contributions are accepted during notice.
  - **During notice** the member stays at risk and keeps earning credits.
  - **After notice,** anyone may queue the exit, but only while the LifeRegistry says the member is alive and checked in, so an heir can't cash out a dead member's at-risk money.
  - **A notice is not an option to hold.** It must be used within 150 days of maturing (longer than a pending recovery or a report's window, which hold exits), and never once income has started (`exit` re-checks both). An independent review showed a member could give notice early, let it sit for years, and leave while being paid.
  - **At the next settlement,** the bequest shares and 99% of the at-risk shares are sold for the member. The other 1% stays in that epoch's credit pool for the members who remain.
  - **A death beats a queued exit.** A death reported before that settlement releases the member as a death, and each member is released once.
- **Paying members can't exit,** the same as with an annuity. β is fixed at joining.
- **Fee:** 0.30% a year of value, accrued per epoch to the protocol treasury. It's the only fee.

## 8. Treasury

- **Sleeves:** cash = USDG in Morpho `steakUSDG`; SGOV and SPY Robinhood stock tokens. Addresses and measured pool depth are in `config/robinhood-mainnet.json`.
- **Glide path (target SPY share by age):** 80% until 40, falling linearly to 30% at 65, flat after. The remainder is split 70/30 between SGOV and cash.
- **Prices:** Chainlink feeds (which include the ERC-8056 multiplier). A price counts only if it updated within 26 hours and the token isn't paused.
- **Settlement and rebalancing run only while all feeds are fresh,** which in practice means US market hours.
  - Feeds went 20–28 hours without an update over the weekend of 2026-09-26. So the pool never settles on a weekend price, and the closed-hours gap problem doesn't arise.
- **Swaps** go through the measured pools (SPY/USDG 0.05%, USDG/SGOV 0.0375%). A swap reverts if its fill is more than 50 bps worse than Chainlink (governance bound: at most 200 bps). The check **limits** manipulation rather than preventing it: a searcher who moves a pool before a permissionless settlement can take up to that bound on each trade, against a price up to 26 hours old. A fresher reference price is on the roadmap.
- **Values while markets are closed.** `lastPrices()` returns the latest oracle prices without the staleness check, with the age of the oldest. It is used only to show members what they hold, never to trade or to price deposits.
- **Depth:** a $10k trade measured at ≤4 bps. QQQ is excluded as too thin.
- **Routes can be replaced, behind the timelock.** If a sleeve's Uniswap v4 pool lost its liquidity, every settlement selling that sleeve would revert. `setRoute` accepts only a pool pairing USDG with the sleeve's own token, with no hooks, and every fill stays bounded against Chainlink. Measured on the fork: a pool nobody created fails the trade; mainnet's second SPY/USDG pool (0.30%) fills $1,000 within the bound (about 21 bps worse than the 0.05% pool); switching back works.
  - **Liveness cost:** a settlement stuck at its sale waits for the fix. Deposits pause, while claims and withdrawals continue, for at least the 48-hour timelock delay.
- **A rebalance never blocks income.** If a rebalance is still planning or trading a day after it started, anyone may abandon it (`abortRebalance`). Nothing has traded in those phases, because the trade step is one atomic transaction. The epoch counts as rebalanced, and settlement goes on. An independent review showed that a sleeve that couldn't trade froze every cohort's income.

## 9. Invariants

Each invariant is listed with where it's enforced today, or marked **not yet built**.

1. **Conservation:** every share that leaves a cohort lands in another cohort, the fee bucket, a bequest, exit or restore claim, a sale order, the carry, or the revival reserve. A top-up never changes income already owed: the new units' income snapshot is set so the held units keep exactly what they had accrued (rounded down), checked over 20,000 random holdings and in the pool (skeptic regression). *Enforced:* exact over 10,000 random settlements and 10,000 random rebalances (`actuary-core`); the pool matches the core ledger exactly in every lifecycle test. *Not yet built:* an on-chain check of `Σ value + claims + fees = Treasury NAV`.
2. **Credits:** `Σ credits + carry = released at-risk shares`, exactly, and each dead member's units reach 0. *Enforced:* `actuary-core` and pool death tests.
3. **Payouts:** no income leaves the pool for a member the LifeRegistry doesn't consider alive, identified and recently strong-proven. The dead can't claim, contribute or exit, and no money enters for a member whose identity isn't verified. *Enforced:* pool and LifeRegistry tests.
4. **Idempotency:** an epoch settles once; a second settlement before the epoch elapses is refused, and each queued member is released once. *Enforced:* pool tests.
5. **Stale prices:** no settlement or rebalance starts on a stale price, and every Treasury trade re-checks freshness and bounds its fill against the oracle. *Enforced:* Treasury fork tests.
6. **Bounded work:** no transaction's gas grows with the number of cohorts, deaths or deposits (§5.1). *Enforced:* by construction, and tested at one item per transaction.
7. **Paging is exact:** any page size gives the same state (§5.1). *Enforced:* core and pool tests.
8. **Rounding:** every truncation favors the pool, never a member. *Enforced:* income booking never pays more than received (`actuary-core`).
9. **Governance bounds:** every parameter moves only within bounds:
   - valuation rate 2–5%, and never below the escalation; escalation 0–4%; SPY return −5–15%; volatility 1–50%; safe rate 0–10%; pool fee 0–1%;
   - epoch 28–31 days; member cap ≤ 100,000 USDG; exit notice 180–730 days;
   - check-in 30–180 days, with the challenge window at least a check-in period plus grace and at most 365 days;
   - staleness 26–96 h and slippage 0.3–2% (never tight enough to stop settlement); the identity period 400–730 days, lengthen only.
   Mortality is sealed and can't change at all. A new identity adapter waits 30 days. Every change goes through a 48-hour OpenZeppelin TimelockController: the deployer proposes, anyone executes once the delay has passed. Both Stylus contracts can be initialised only by the deployer address compiled into them, so nobody can front-run the set-up. *Enforced:* bounded setters and the seal (Actuary, pool and LifeRegistry tests); the timelock flow, including that it can't bypass a contract's bounds (`test/Governance.t.sol`).
11. **Tests that can fail:** the pool's test VM reverts on any external call a test didn't set up, so a wrong recipient, amount or key fails. Planted bugs, one for each flaw found by the four reviews and each key rule, are each caught: 36 in the pool (`engine/tests/mutate_pool.py`, `runs/mutations-pool.json`) and 23 in the LifeRegistry (`engine/tests/mutate_registry.py`, `runs/mutations-registry.json`).
10. **Pause:** a guardian can pause instantly, but can't unpause, move funds or change balances. Pausing stops joins and deposits until unpaused, and holds exits and the start of settlements and rebalances for **at most 7 days**; the 7 days count from when the pause began (pausing again changes nothing), and nobody, the owner included, can pause more than once in 30 days, so no single key can freeze income. Claims, withdrawals and death reports go on, and a settlement already under way may finish. *Enforced:* pool test, and a mutation holding settlements for the whole pause fails it.

## 10. Evidence we publish (not assert)

- **Mortality fit:** error per `(country, sex, birthYear)`.
- **Fixed-point math:** error bounds from property tests against a high-precision reference.
- **Fairness error** by pool size (100, 1k, 10k members) from the population simulator.
- **Ghost detector:** power and false-alarm rate across 10 seeds, with realistic reporting lags (`runs/ghost-detector.json`).
- **Historical replay** (`actuarial/replay.py`, money flows through the production ledger via `actuary-cli replay`; report at `runs/replay.json`):
  - **Setup.** 2,000 Philippine women with $10,000 each retire at 65. Deaths are drawn from the cohort's actual UN death rates, year by year. Returns are the CRSP market total return and 1-month T-bills (Fama-French), in the 65+ mix (30% SPY, 49% T-bills, 21% cash), rebalanced monthly. The pool pays the 0.30% fee and the trading costs measured on the mainnet fork. The solo benchmarks trade for free and pay no fee.
  - **No hindsight.** Payouts are priced from that year's period life table at 3.5%, the table an actuary would have had then.
  - **Pricing instead from the cohort's realized rates** (perfect foresight) moves income by at most 7.3% in any year while 100 or more members are alive, with one exception. In 2021, COVID-19 pushed the UN period death rate of Philippine women aged 85 from 0.144 to 0.224. Pricing from that year's table assumed pandemic mortality for life and paid 39% more that year, which is survivors' future income paid early.
    - Foresight pricing let the year's extra deaths reach survivors only as mortality credits: income rose 9% in nominal terms.
    - The Actuary contract behaves like the foresight run, not the period run. It prices from each cohort's fitted curve, not from the latest period table, so a one-year shock pays out through credits and doesn't reprice the rest of life.
  - **Real income** is in retirement-year dollars (Shiller CPI-U):

    | Retired | Level plan, yr 1 → yr 10 → yr 20 (lowest) | Escalating plan, yr 1 (lowest) | The pool's income drawn alone ran out | 4% rule ($400 a year, real) |
    |---|---|---|---|---|
    | 1965 | $1,013 → $684 → $592 ($550) | $881 ($651) | 1977, **49% of the cohort still alive** | ran out 1996, 0.2% alive |
    | 1973 | $926 → $630 → $748 ($582) | $803 ($640) | 1985, **51% alive** | ran out 2006, 0.3% alive |
    | 2000 | $882 → $570 → $484 ($434, in 2022) | $757 ($572) | 2014, **52% alive** | lasting in 2023 |

  - **What it shows.**
    - Drawn alone, the same month-by-month income, from the same money with the same investments and no fees, ran out with about half the cohort still alive, in all three cohorts. Across 20 other death draws, that share stayed within 48–54%.
    - While 100 or more of the 2,000 were alive, the pool paid more than the 4% rule in every year of every cohort, in all 21 draws of who died when. The closest was the 2000 cohort in 2022: $427–$529 against $400. The 4% rule is the standard safe drawdown: it lasts, and leaves a bequest, by paying less.
    - With fewer than 15 members left (ages 99–100), one cohort's credits become lumpy, and in 2 of the 21 draws (both for the 1973 cohort) a year fell below $400 (lowest $379). In the production pool, credits come from deaths in every cohort (§3). This one-cohort replay doesn't model that.
    - Against the pool's own payout rule applied without pooling, pooled income was 1.3–1.5× higher at year 10 and 2.9–4.3× at year 20. That gap is the mortality credits.
  - **What it doesn't hide.** Income isn't guaranteed and is paid in nominal USDG.
    - From 2000 to 2023 the 30/70 mix earned 3.4% a year nominal, against the 3.5% plus 0.30% fee it was priced at, while inflation ran at 2.6%. The level plan's real income fell 51% over 23 years, ending in 2022's 8% inflation and −19% stock market; the escalating plan's fell 24%.
    - In 1965–75 stagflation, level real income fell 32% in ten years.
    - The escalating plan exists for this. So does setting `r` from the safe sleeve's actual yield, which governance can do within its bounds.
- **Cost** (`engine/ink-meter`: the deployable WASM, instrumented with nitro's `pricing_v1` per-opcode ink, 2,450-ink block headers and Stylus v3 host-function prices; storage by EIP-2929/2200; the Solidity Treasury and LifeRegistry charged their Foundry-measured mainnet-fork gas):
  - quote: table in §4.2
  - `payoutFraction` 150k gas, `qMonth` 37k gas
  - a month for 300 members in 300 cohorts, with 6 deaths and the ghost detector: settlement 19 transactions and 49.5M gas, rebalance 13 transactions and 40.7M gas, at pages of 50 (dearest page 5.8M gas). That's **301k gas per member-month, about $0.033** at 0.027 gwei and $4,000/ETH. Members sharing cohorts share the cost.
  - these are lower bounds by each program's activation-specific init gas, known only after deployment, and they'll be re-measured with `cast estimate` once live
