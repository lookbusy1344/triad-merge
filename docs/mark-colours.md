# Mark colours and shapes

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
| H · Tide | Light | `#0F9488` | `#E8603C` | `#1F2430` | Teal and coral arms with an ink stem. |
| | Dark | `#2DD4BF` | `#FF8A65` | `#F2F3F5` | |
| I · Primary triad | Light | `#1D4ED8` | `#E8481C` | `#E3A008` | Blue, red-orange and yellow. |
| | Dark | `#5B8DEF` | `#FF6B3D` | `#FFC83D` | |

## Geometry in use

The mark sits on a 256-unit grid. Each piece is filled and stroked in its own colour, stroke width 10, round joins, which rounds the corners.

| Piece | SVG path |
|---|---|
| Left arm | `M32 42H118V130L32 84Z` |
| Right arm | `M224 42H138V130L224 84Z` |
| Stem | `M98 142L128 158L158 142V214H98Z` |

The ink, stroke included, spans x 27–229 and y 37–219. The icon puts the mark at 80% scale, centred on a 256-unit plate with corner radius 56. The macOS icon follows Apple's grid: the plate fills 824 of 1024 units with corner radius 185.4.

The name is always lowercase `triad`, in the wordmark and wherever the logo shows it. The wordmark is Schibsted Grotesk SemiBold (600), letter spacing −0.02 em, kerned and outlined at 284 units so no file needs the font. Its outline, origin on the baseline at the left edge, 596 units wide:

```svg
<path fill="TEXT" d="M59.2 0Q45.6 0 37.6 -7.3Q29.5 -14.6 29.5 -27.3L29.5 -123.7L7.1 -123.7L7.1 -149.8L29.5 -149.8L29.5 -179.9L63.8 -187.9L63.8 -149.8L92.4 -149.8L92.4 -123.7L63.8 -123.7L63.8 -33.8Q63.8 -29.1 65.1 -27.6Q66.4 -26.1 70.7 -26.1L93.6 -26.1L93.6 0ZM118.7 0L118.7 -149.8L151.6 -149.8L151.6 -116.8L150.3 -120.9Q157.3 -137.1 170.3 -145.2Q183.3 -153.2 198 -153.2Q202.2 -153.2 206.4 -152.5Q210.6 -151.8 214.5 -150.2L211.5 -120.1Q203 -122.6 195 -122.6Q189 -122.6 181.7 -120.4Q174.5 -118.3 167.9 -112.5Q161.4 -106.6 157.2 -95.8Q153 -84.9 153 -67.5L153 0ZM234.4 0L234.4 -149.8L268.6 -149.8L268.6 0ZM251.4 -170.4Q240.7 -170.4 234 -177.2Q227.2 -184 227.2 -194.7Q227.2 -205.5 234 -212.2Q240.7 -219 251.4 -219Q262.2 -219 269 -212.2Q275.7 -205.5 275.7 -194.7Q275.7 -184 269 -177.2Q262.2 -170.4 251.4 -170.4ZM345.4 3.5Q330.9 3.5 319.2 -1.8Q307.6 -7.1 300.8 -17.1Q294 -27.2 294 -41.2Q294 -56.3 302.8 -67.6Q311.6 -78.9 328.9 -85.1Q346.1 -91.4 371.7 -91.4L394.1 -91.4L391.6 -88.1L391.6 -96.5Q391.6 -112.6 385.5 -120.8Q379.4 -129 365.3 -129Q351.8 -129 344.6 -121.9Q337.4 -114.8 336.3 -102.3L300.4 -109.1Q302.2 -121.5 310.4 -131.5Q318.7 -141.4 333 -147.4Q347.3 -153.4 367.4 -153.4Q386.5 -153.4 399.4 -146.9Q412.3 -140.3 418.7 -126.7Q425.2 -113 425.2 -91.4L425.2 -33.8Q425.2 -29.1 426.5 -27.6Q427.8 -26.1 432.1 -26.1L439.6 -26.1L439.6 0L421.7 0Q408.1 0 400.5 -7.3Q392.9 -14.6 392.9 -27.3L392.9 -35.8L398.7 -34.1Q396.8 -28.2 393 -21.6Q389.3 -15.1 383 -9.4Q376.8 -3.7 367.6 -0.1Q358.3 3.5 345.4 3.5ZM354.9 -21.4Q365.3 -21.4 373.7 -26.3Q382.1 -31.2 387 -41.4Q391.9 -51.6 391.9 -67.1L391.9 -69.6L372.2 -69.6Q356 -69.6 346.8 -65.4Q337.7 -61.2 333.9 -55Q330.2 -48.8 330.2 -42.7Q330.2 -37.4 332.6 -32.5Q335 -27.6 340.5 -24.5Q345.9 -21.4 354.9 -21.4ZM514.2 3.5Q495.8 3.5 481.8 -6Q467.9 -15.4 460.3 -33.1Q452.8 -50.8 452.8 -75.2Q452.8 -99.6 460.4 -117Q468 -134.5 482 -143.9Q495.9 -153.2 514.6 -153.2Q530.4 -153.2 544 -145.7Q557.6 -138.3 565.4 -120.8L561.8 -120.2L561.8 -208L596.2 -208L596.2 0L563.6 0L563.6 -30.5L564.1 -28.4Q557.2 -12.1 543.8 -4.3Q530.4 3.5 514.2 3.5ZM525.9 -23.2Q536.4 -23.2 544.9 -28.7Q553.3 -34.3 558.3 -45.7Q563.3 -57.1 563.3 -74.6Q563.3 -100.8 552.5 -113.7Q541.7 -126.6 526.1 -126.6Q509.8 -126.6 499.6 -112.9Q489.4 -99.3 489.4 -74.3Q489.4 -50.2 499.2 -36.7Q509.1 -23.2 525.9 -23.2Z"/>
```

In the logo the wordmark sits 38 units right of the mark's ink, scaled 0.83, its baseline on the ink's bottom edge (y 219). The logo's viewBox is `27 37 734.68 186`: the ink plus the wordmark, with 4 units below for the overshoot of the round letters.

## Other shapes

Four other shapes were drawn on the same 256-unit grid. Each has a left input, a right input and a result. They share one palette:

| Theme | Left | Right | Result |
|---|---|---|---|
| Light | `#2273C3` | `#C32273` | `#8022C3` |
| Dark | `#67A6E4` | `#E467A6` | `#B067E4` |

The monogram, the shape in use, was first drawn in this palette.

**A · Panes.** Three panes: two squares above one wide bar, gutter 12, inner corners radius 12, outer corners radius 52.

```svg
<path fill="LEFT" d="M68 16H110A12 12 0 0 1 122 28V110A12 12 0 0 1 110 122H28A12 12 0 0 1 16 110V68A52 52 0 0 1 68 16Z"/>
<path fill="RIGHT" d="M146 16H188A52 52 0 0 1 240 68V110A12 12 0 0 1 228 122H146A12 12 0 0 1 134 110V28A12 12 0 0 1 146 16Z"/>
<path fill="RESULT" d="M28 134H228A12 12 0 0 1 240 146V188A52 52 0 0 1 188 240H68A52 52 0 0 1 16 188V146A12 12 0 0 1 28 134Z"/>
```

**B · Confluence.** Two bands curve in from the top corners, cross, and join a stem at the bottom.

```svg
<path fill="LEFT" d="M24 16H80V56C80 120 156 112 156 176H100C100 112 24 120 24 56Z"/>
<path fill="RIGHT" d="M232 16H176V56C176 120 100 112 100 176H156C156 112 232 120 232 56Z"/>
<path fill="RESULT" d="M128 123.6C143.2 135.6 156 149.7 156 176V240H100V176C100 149.7 112.8 135.6 128 123.6Z"/>
```

**D · Lines.** Three rounded bars: left-aligned, right-aligned, then full width.

```svg
<rect fill="LEFT" x="24" y="30" width="140" height="52" rx="16"/>
<rect fill="RIGHT" x="92" y="102" width="140" height="52" rx="16"/>
<rect fill="RESULT" x="24" y="174" width="208" height="52" rx="16"/>
```

**E · Overlap.** Two tall rounded rectangles overlap; the result fills the overlap.

```svg
<rect fill="LEFT" x="24" y="40" width="140" height="176" rx="36"/>
<rect fill="RIGHT" x="92" y="40" width="140" height="176" rx="36"/>
<rect fill="RESULT" x="92" y="40" width="72" height="176" rx="36"/>
```

Each wraps in `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">`.

## Presentation boards

The boards that compared these designs show each mark at 240, 64, 32 and 16 px, and beside the wordmark at 72 px mark to 80 px text. Light boards use background `#EEEEF1`, text `#1F2430` and a `#FFFFFF` plate. Dark boards use background `#0F1115`, text `#F2F3F5` and a `#1F2430` plate. A plate's corner radius is 22.5% of its side. On the shape boards the mark fills 74% of the plate; on the colour boards, 80%, as in the icon. Shape A has no plate. The colour boards show each option at 128, 32 and 16 px, and beside a 56 px wordmark.
