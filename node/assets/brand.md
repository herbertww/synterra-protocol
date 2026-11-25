# Synterra Core: brand

**Synterra Core is in build. Nothing has shipped.** This guide covers the node's name, mark, colour, type and voice.

## The idea

Mining has always ended at the assay office. Ore comes in from anyone. The assayer tests it, and metal that passes is struck with a hallmark, a small punched mark that anyone can read and nobody can fake.

Synterra Core is the assay office for proof mining. Anyone mines. The node assays each contribution with Lean's own judge, `lake comparator --paranoid`, which checks it against Lean's kernel and every external checker bundled with Lean. A passing contribution gets a signed attestation, which is its hallmark.

**Tagline:** *Mined by anyone. Assayed by the kernel.*

## The name

**Synterra Core.** The binary is `synterra`.

It names the reference node in the line of Bitcoin Core (BitTorrent → Bitcoin → Synterra), and it tells a security reviewer exactly what they are installing.

**Why not "SYNful" for the software:**

1. **SYNful Knock** is a well-known 2015 Cisco router backdoor, disclosed by Mandiant and Cisco. Software you are asked to download and run under a name one letter-case away from a router implant starts with a trust deficit that security reviewers will notice first.
2. **"SYNful" already has a job.** The cemented rules use it for a lemma that earns SYN. One word, one meaning.

**Use SYNful the way the rules do.** Ask "Is this lemma SYNful?", not "Run SYNful".

## The mark

The logical turnstile **⊢** ("proves") struck into a hallmark cartouche: a square punch with concave, notched corners, the shape assay offices strike into silver.

| File | Use |
|---|---|
| `mark.svg` | Default. Cobalt, switches to light cobalt when the viewer prefers dark. Use for favicons and anywhere one file must serve both themes. |
| `mark-light.svg` | Cobalt `#2347C6`, for light grounds. |
| `mark-dark.svg` | Light cobalt `#7E9CFF`, for dark grounds. |
| `mark-sizes.png` | Proof sheet from 16px to 256px on both grounds, plus the 16px raster at 8×. |

**Construction** (64-unit grid): the cartouche spans 4–60 with corner notches of radius 8. The turnstile's stem is x 20–28, y 16–48, and its arm is x 28–48, y 28–36, so the stroke is 8. All edges land on multiples of 4, so the 16px favicon stays pixel-crisp. The turnstile is a hole, not a shape: whatever is behind the mark shows through it, as the engraving does in the banner.

**Clear space:** 16 units on every side, a quarter of the mark's width. **Minimum size:** 16px.

**Don't:**
- outline it;
- add gradients or bevels to the small mark;
- rotate it;
- recolour it outside the palette;
- fill the turnstile;
- put cobalt `#2347C6` on ink (2.61:1, too dark);
- crown it with horns, flames or any devil imagery.

The struck, raking-light treatment on the brand sheet is for large display only.

## Colour

| Token | Hex | Role |
|---|---|---|
| ink | `#0B0B0D` | Dark ground |
| paper | `#F1EFEA` | Light ground; text on ink |
| cobalt | `#2347C6` | Mark on light; fills; the ACCEPTED chip |
| light cobalt | `#7E9CFF` | Mark and accents on dark |
| muted | `#8E8C87` | Secondary text on ink only |
| alarm | `#B3261E` | The REJECTED chip only |

**Measured contrast (WCAG):**

| Pair | Ratio | Verdict |
|---|---|---|
| paper on ink | 17.11 | Body text |
| ink-2 `#C9C7C1` on ink | 11.63 | Secondary text |
| light cobalt on ink | 7.57 | Mark, links, accents |
| cobalt on paper | 6.55 | Mark, links, accents |
| paper on cobalt | 6.55 | Text on cobalt fills |
| paper on alarm | 5.69 | Text on the REJECTED chip |
| muted on ink | 5.85 | Captions on ink |
| dim `#5F5D59` on paper | 5.72 | Captions on paper |
| muted on paper | 2.92 | Don't use for text |
| cobalt on ink | 2.61 | Don't use |
| light cobalt on paper | 2.26 | Don't use |

Terminal colours have their own table in `terminal.md`, tuned to read on both light and dark terminals.

**The engraving.** A fine moiré of crossing sine lines, the underprint of a banknote or an assay certificate, in light cobalt on ink or cobalt on paper at low opacity. It belongs behind the mark and on attestation cards, faded out before any text. Never put it behind body text.

## Type

| Face | Weight | Use |
|---|---|---|
| Manrope | 800 | Wordmark and headlines. Tracking −0.05em at display size. |
| Manrope | 500 | Taglines and body. |
| JetBrains Mono | 400 / 500 | The CLI, ids, hashes, code, captions that quote a command. |

The wordmark is "Synterra Core" set in Manrope 800, in one colour: paper on ink, ink on paper. It is set in type, not drawn, so outline it before it goes anywhere without web fonts.

## Words

| Word | Means | Where |
|---|---|---|
| **assay** | To check a contribution with Lean's judge | Prose and marketing. The CLI command stays `check`. |
| **hallmark** | A signed attestation that a contribution passed | Prose and marketing. The protocol word stays `attestation`. |
| **pass / fail / skipped** | One gate's result | CLI and docs |
| **advisory** | A pass that cannot yet be fully trusted | CLI and docs; always printed |
| **ACCEPTED / REJECTED** | One node's verdict on one submission | The CLI chip; capitals only there |
| **SYNful** | A lemma that earns SYN: it passed every gate and scored in the upper band of its period | Never for one node's verdict. Accepted is not SYNful. |

## Voice

Wild in voice, clean in proof. The nation can be loud. The checker can't.

**Do:**
- Name the exact check: "Lean's kernel plus every external checker bundled with Lean".
- Say what has not shipped.
- Quote real commands.
- Let the mark and one strong line carry the page.

**Don't:**
- Call anything unhackable, unbreakable or trustless.
- Invent users, nodes, uptime or SYN figures.
- Promise yield.
- Use devil imagery, pentagrams, horns or flames.
- Use "SYNful" for the binary.
