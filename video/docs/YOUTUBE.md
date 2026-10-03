# Tonti demo video: YouTube title, description, thumbnail

## Title (88 characters; YouTube allows 100, and search shows about the first 60)

Tonti: Income for Life, Priced On-Chain | Robinhood Chain · Arbitrum Open House Singapore

*Alternative, question-led:* How much would your mother get, every month, for life? | Tonti on Robinhood Chain

## Thumbnail

- **Use:** `/media/dflame/UNIQ/arbit/video/out/thumbnail-ThumbA.png`: "$67.67 a month, for life" on the dusk sky. 1280×720 PNG, under YouTube's 2 MB limit.
- **Alternative:** `thumbnail-ThumbB.png`: "Income for life, priced on-chain." beside the live site's answer.

## Description (paste everything below the line)

---

How much would your mother get, every month, for the rest of her life? Tonti asks Robinhood Chain, and a contract answers in about a second.

Singapore employs 1,635,700 foreign workers (Ministry of Manpower, December 2025). None of them can save into CPF, the city's pension, and most send money home to parents who have no pension either: only 24% of older people in South Asia receive one (ILO). Tonti is a pension pool that pays its members an income for life, and it runs entirely on Robinhood Chain.

▶ Try it: https://tonti-life.vercel.app (English and Filipino)
▶ Code: https://github.com/iamdflame/tonti

CHAPTERS
0:00 1.6 million workers, no pension
0:25 One question, answered by the chain
0:58 Run it yourself
1:08 How it works
1:36 We replayed history
1:57 It's live: the first member
2:30 Nobody can take the pot
2:52 Habambuhay: for life

WHAT YOU'RE SEEING IS REAL
• The quote is a single eth_call to the Actuary contract (Arbitrum Stylus, Rust). It simulates 512 possible lives with real market assumptions, in about a second, and anyone can re-run it on a public node and get the same answer.
• Members are pooled by country, sex and birth year: 1,846 cohorts in 13 countries, priced from UN World Population Prospects 2024 and sealed on-chain, so no one can change them.
• Savings are invested in S&P 500 (SPY) and US T-bill (SGOV) tokens on Robinhood Chain, and in USDG lent in Morpho. When a member dies, the savings they put at risk are shared among those still alive: that is what makes the income last a lifetime.
• History replay: 2,000 Filipino women retiring in 1965 with $10,000 each, through the real markets that followed. Drawn alone, their money ran out in 1977 with half of them still alive. The pool paid every one of them for as long as they lived.
• Live on mainnet: the first member joined from an iPhone, checked in with Face ID (a passkey verified on-chain), passed the identity check and paid in 25 USDG. The first monthly settlement invested it. Every number in the video is read from the chain.

SAFEGUARDS
• Mortality tables sealed: no one can change them, not even us.
• Every rule change waits 48 hours in public (timelock).
• A false death report is answered by a single check-in, and any death can be undone for five years.
• 59 bugs deliberately planted in our own contracts: the tests caught every one. Four independent adversarial reviews.

CONTRACTS (Robinhood Chain mainnet)
Actuary: https://robinhoodchain.blockscout.com/address/0x1464069e9f1110e6f0f52231a2c1f87c2fc9ccaa
TontiPool: https://robinhoodchain.blockscout.com/address/0xfde46333804f2ea5167bdb7f8f7408ab0cee6308

Built for HackQuest's Arbitrum Open House Singapore Online Buildathon.

Research preview: deposits are capped at 25 USDG per member. Not financial advice. Not an insurance product.

Footage: Marina Bay Sands SkyPark view by LN9267 and Tampines West time-lapse by vicsonhuang, Wikimedia Commons, CC BY 3.0. Sunset: Mixkit (free licence). Voice, music and sound: ElevenLabs.

#Arbitrum #RobinhoodChain #Stylus #DeFi #Pension #USDG

---

**If you change the edit's timing,** move the chapter times to match: YouTube needs the first one at 0:00, at least three, and each at least 10 seconds long.
