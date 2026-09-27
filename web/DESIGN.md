# Tonti: design system ("Lifeline at dusk")

This file is the source of truth for every screen. If a component disagrees with it, the
component is wrong.

## Who it's for

| Reader | Situation | What they need |
|---|---|---|
| **Migrant workers in Singapore** | 25–45, mostly Filipino and Indonesian domestic workers or South Asian construction workers. Budget Android, one hand, Messenger and WhatsApp. | Payments that feel familiar: you pay X, she gets Y |
| **Their parents** | 55–80, back home. Low vision, Filipino first. | One big button every three months |
| **Hackathon judges** | Desktop, sceptical. | Proof in seconds that the numbers come from the chain |

## Idea

A life drawn as one day, over Manila Bay, whose sunsets Filipinos know.
- The sky's light follows the member's own survival curve, read from the chain.
- Income is a lamp.
  - Saved alone, the lamp goes out while the sun is still up.
  - The pool's lamp stays lit until night.

## Colour

Tokens in `src/app/globals.css`. Components use only these tokens.

| Token | Hex | Use |
|---|---|---|
| `night` | `#0D1A30` | deepest sky, footer |
| `bay` | `#13233F` | dark surfaces, upper dusk sky |
| `bay-2` | `#1C3157` | raised surfaces on dark |
| `haze` | `#EEF2F6` | light surfaces: a cool paper, **never cream** |
| `paper` | `#FFFFFF` | cards on haze |
| `ink` | `#14213D` | text on light |
| `ink-2` | `#4B5873` | secondary text on light (6.5:1 on haze) |
| `mist` | `#A9B4C6` | secondary text on dark (7:1 on bay) |
| **`lamp`** | `#FFB23F` | income figures and the one primary button per view; nothing else |
| `coral` | `#F2785C` | the sky's horizon band only; never UI state |
| `ok` | `#1F7A4D` | success, always with an icon and a word |
| `danger` | `#B42318` | destructive actions only, always with an icon and a word |

**Contrast:**
- ≥ 4.5:1 for body text.
- ≥ 7:1 for money.
- 3:1 for controls and focus rings.

On light surfaces, lamp is never text. The primary button there is lamp with ink text (8.9:1).

## Type

| Font | Used for |
|---|---|
| **Atkinson Hyperlegible Next** | UI and headlines, weights 400–800 |
| **Atkinson Hyperlegible Mono** | amounts, addresses, hashes, block numbers; tabular figures |
| **Kalam** (via Tegaki) | one handwritten phrase under the income: *for life* / *habambuhay* |

Atkinson Hyperlegible is the Braille Institute's typeface for low-vision readers, and the people this is for are 55–80 on budget phones.

**Scale:** 5 steps, each ≥ 1.2× the last.

| Step | Size | Line-height | Letter-spacing |
|---|---|---|---|
| display | `clamp(2.6rem, 5.2vw + 1rem, 5.25rem)` | 1.03 | −0.02em |
| title | `clamp(1.9rem, 2.6vw + 1rem, 3rem)` | 1.08 | −0.015em |
| heading | 1.5rem | 1.2 | 0 |
| body-l | 1.25rem | 1.5 | 0 |
| body | 1.0625rem | 1.55 | 0 |

**Text rules:**
- Captions only are 0.9375rem, never below 16px on mobile for anything a person must read.
- Lines are 60–75 characters.
- Headings use `text-wrap: balance`; paragraphs use `pretty`.

## Space, shape, depth

- **Spacing:** 4/8 px steps. Space between groups is at least twice the space inside a group.
- **Radius:**
  - 12 px for controls, 20 px for cards, 28 px for sheets.
  - Nested radius = inner + padding.
- **Shadows:** Meng To's three recipes as tokens (`shadow-sm`, `shadow-md`, `shadow-lg`).
  - Never tinted.
  - Cards use a 1 px ring as their edge.
  - No hover lift on cards.
- **Targets:** 48 px on anything a parent touches; 44 px elsewhere; 12 px between filled controls.
- **Page edge:** 16 px gutters. Safe-area insets respected.

## Motion

| Token | Value |
|---|---|
| `--ease-out` | `cubic-bezier(0.23, 1, 0.32, 1)` (enter, exit) |
| `--ease-move` | `cubic-bezier(0.77, 0, 0.175, 1)` (moving on screen) |
| `--ease-sheet` | `cubic-bezier(0.32, 0.72, 0, 1)` (drawers) |

**Durations:**

| Use | Duration |
|---|---|
| press | 120 ms (scale 0.97) |
| hover | 160 ms (fine pointers only) |
| menus | 200 ms |
| sheets | 320 ms |
| the reveal | 900 ms |

**The one orchestrated moment:** the quote's answer arriving.
- The sun settles on her curve.
- The number slides into place.
- *For life* writes itself.
- The two lamps draw once.

Nothing else animates on load.

**Motion rules:**
- Springs have bounce 0.
- Everything is interruptible.
- Only `transform` and `opacity` animate.
- Reduced motion swaps every movement for a ≤ 200 ms fade.
- Keyboard actions never animate.

## Banned (the stock tells)

- purple-blue gradients, gradient text, glass as decoration
- Inter, Geist, cream-and-terracotta with a serif
- three identical feature cards; cards inside cards
- ALL-CAPS eyebrow labels; emoji headers; an arrow on every button
- a big number with a gradient accent as the whole hero; fade-up on every section
- "Powered by AI", "seamless", "revolutionary"

## Words

**Plain words:** "monthly income", "for life", "pay in", "check in". Never "vault", "staking", "yield farming", "APY".

**Buttons:**
- Start with a verb.
- A button keeps its name into the confirmation: "Pay in 25 USDG" → "Paid in 25 USDG".

**Errors:**
- Say what happened and what to do, calmly.
- No "oops".

**Sentences:**
- Always whole sentences with placeholders, so Filipino can reorder them.
- Money is formatted with `Intl` for the locale (`en`, `fil-PH`).

**Honesty is part of the design:**
- Every chain figure shows where it came from: a block, a contract, "Run it yourself".
- The preview's limits are said plainly.

## Screens

| Route | Job |
|---|---|
| `/` | The question, the chain's answer, why it holds, how to start |
| `/q/[slug]` | A shareable quote and its image |
| `/join` | For me / for my parent; the parent's invite (`/i`) |
| `/me` | Every account a wallet is part of, by lifecycle state |
| `/checkin/[id]` | One button for a parent |
| `/pool` | Live proof |
| `/operator` | The identity attester's desk |

## Checks before release

- **axe:** 0 violations.
- **Lighthouse (mobile):** Performance ≥ 90, Accessibility 100.
- **Screenshots:** 320, 375, 768 and 1440 px, English and Filipino.
- **Design review:** design-review and better-interface on screenshots. Any HIGH finding blocks release.
