# Mark colours

The mark is a T in three pieces: the left arm, the right arm and the stem. The two arms are the inputs to a merge, and the stem is its result. Each colour scheme has one set of colours for light backgrounds and one for dark backgrounds. The icon's plate is `#F7F7F9`. The wordmark is `#1F2430` on light and `#F2F3F5` on dark.

`script/gen-images.swift` holds the scheme in use, as `onLight` and `onDark`. To switch schemes, copy a row's values there and run `swift script/gen-images.swift`.

## In use

G6 · Graphite and signal: a near-black arm and a light grey arm, with a saturated green stem. The large difference in lightness keeps the two arms apart at 16 px.

| Theme | Left arm | Right arm | Stem |
|---|---|---|---|
| Light | `#2A2F3A` | `#A9B0BC` | `#00B140` |
| Dark | `#E4E7EC` | `#5A6170` | `#3EE07A` |

## Other options

| Option | Theme | Left arm | Right arm | Stem | Notes |
|---|---|---|---|---|---|
| F · Overprint | Light | `#E3A008` | `#0891B2` | `#15803D` | Yellow and cyan arms, with the green they overprint to. |
| | Dark | `#FFC83D` | `#35C4E8` | `#5BD68A` | |
| G · Resolved | Light | `#434957` | `#747A86` | `#6BAA35` | The greys are 13% apart in lightness and merge below 32 px. |
| | Dark | `#8E939D` | `#C3C6CD` | `#B6E05A` | |
| G1 · Ink and silver | Light | `#1F2430` | `#A3ABB9` | `#16A34A` | The left arm is the wordmark's ink. |
| | Dark | `#F2F3F5` | `#6B7588` | `#4ADE80` | |
| G2 · Steel and lime | Light | `#334155` | `#94A3B8` | `#65A30D` | Blue-tinted greys with a yellow-green stem. |
| | Dark | `#CBD5E1` | `#64748B` | `#A3E635` | |
| G3 · Slate and stone | Light | `#3B4A63` | `#AFA59A` | `#059669` | A cool grey and a warm grey. |
| | Dark | `#9DB0CC` | `#D9CBBE` | `#34D399` | |
| G4 · Dusk | Light | `#3D6A99` | `#9C5C82` | `#1E9E55` | Greys tinted towards the app's left (210°) and right (330°) hues, with its resolved green (140°). |
| | Dark | `#7FAEDC` | `#D58FB8` | `#4FD68A` | |
| G5 · Navy and mist | Light | `#1E3A5F` | `#8EA9C8` | `#10B981` | One blue at two depths. |
| | Dark | `#C3D6EC` | `#4F6F94` | `#34D399` | |
| G7 · Solid and outline | Light | `#1F2430` | `#1F2430` | `#16A34A` | The right arm is drawn as an outline at stroke 18. `gen-images.swift` draws filled shapes only, so this option needs a change to its geometry. The outline closes up at 16 px. |
| | Dark | `#F2F3F5` | `#F2F3F5` | `#4ADE80` | |
