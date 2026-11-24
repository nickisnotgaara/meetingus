# Component Specs — Clean-room measurements

All sizes in **logical px** (screenshots measured at 2x; raw/2). Colors from direct pixel sampling.
`estimate` = not directly measurable from the 6 refs (hover/disabled states, motion, etc.).
Refs: `s01` dashboard, `s02` chat history, `s04` compact widget, `s04b` expanded widget, `s05` screenshot preview, `s07` widget + Use Screen toggle.

## Widget nav bar (s04b, s05, s07)

| Variant | Left | Center/Active | Right | Measured size |
|---|---|---|---|---|
| Full (s04b/s07) | home icon 24, gray `#9c9da1` | `Chat` active pill + `Transcript` text | expand/resize icon 18 | row inset: left 22, top 13; pill 50×27, radius full, bg `#2e2e33`, text 14/600 `#ededf0`; inactive text 14 `#8e8e93` |
| Minimal (s05) | home icon only | same | resize icon | same metrics |

| State | Spec |
|---|---|
| default (inactive tab) | text 14 `#8e8e93`, transparent bg — estimate hover: bg `rgba(255,255,255,0.06)` |
| active | pill bg `#2e2e33`, text `#ededf0` (measured) |
| disabled / loading | not observed — estimate: opacity 0.4 |

Spacing: nav row sits 13 below panel top, 8 gap between home icon and Chat pill, 12 between tabs.

## Action button row — Assist / What should I say next? / Follow-up questions / Recap (s04, s04b, s07)

| Variant | Items | Measured |
|---|---|---|
| Full 4-item (s04, s04b, s07) | ✨ Assist · 🪄 What should I say next? · 💬 Follow-up questions · ⏱ Recap | text 13–14/500 `#cacace` (sampled); icon 16; dot separator 4px `#4a4a52` (estimate); item gap 24; row height ~28 |
| Compact 2-item (s05) | 🪄 What should I say? · 💬 Follow-up questions | same tokens; used when answer is short / screenshot attached |

| State | Spec |
|---|---|
| default | text `#cacace`, icon same (measured) |
| hover | estimate: text `#ffffff`, subtle bg pill `rgba(255,255,255,0.06)`, radius full |
| active/pressed | estimate: bg `rgba(255,255,255,0.10)` |
| disabled | estimate: opacity 0.35 |
| loading | estimate: spinner replaces icon, text unchanged |

Spacing: sits 8–12 above input box; full-width row, left-aligned with panel padding 20.

## Chat input with Smart toggle (s04b, s05, s07)

| Part | Measured spec |
|---|---|
| Container | radius 12, border 1px `rgba(255,255,255,0.08)` (estimate), bg `#16161c`/transparent over glass; width = panel − 2×20 padding; height ~104 (entry 56 + divider + toolbar 44) |
| Placeholder | 15 `#8a8a90`: "Ask about your screen or conversation, or [⏎] for Assist" (s05) / "Ask, [⌘][↵] to start typing" (s07) — keycaps 24×24, radius 6, border `#3a3a40` |
| Divider | 1px `rgba(255,255,255,0.07)` (estimate), full width of input |
| Use Screen pill (s07, ON) | 88×27, radius full, bg `#1a2e4c`, text/icon blue (estimate `#5b9cf5`), border `#2f6ad8` (estimate), image icon 16 |
| Use Screen (s05, ON variant) | blue text + blue tinted bg (measured `#1a2e4c`) |
| Smart pill (OFF, s04/s05) | 67×27, radius full, bg `#1e1e24`, border `#333338` (estimate), bolt icon + text 14 `#c8c8cd` |
| Smart pill (ON, s07) | 78×30, radius full, bg `#c8b850` (measured gold), text white (AA sampled `#fff085`; estimate) |
| Mode label | "Cluely for Sales (Roy) ⌄" 14 `#8a8a90` + chevron (s07) |
| Send button | circle 33–36, bg `#033484`/`#03388c` (measured), white arrow icon 16 |

| State | Spec |
|---|---|
| default (empty) | placeholder shown, send bg `#033484` |
| focused / typing | estimate: border `rgba(255,255,255,0.16)`, send brightens to `#0a4bb8` |
| disabled | estimate: send bg `#1c1c22`, icon `#55555c` |
| loading (sending) | estimate: send shows spinner, controls disabled |

## Streaming answer block (s04b, s07)

| Part | Measured spec |
|---|---|
| Answer text | 15/20 (line pitch 40 raw), `#ededf0`, left-aligned, panel padding 20 |
| Fade mask | bottom ~20: text fades toward transparent/gray (`#6a6a70`) — confirms scrollable stream (s04b) |
| Scroll-down control | circle 33, border `#3a3a40`, ↓ icon, right edge inset ~20, sits at stream bottom-right |
| Loading/thinking row (s07) | 💬 icon 18 + "Thinking about your question" 15 `#ededf0`; partial next line rendered at ~40% opacity then cut off (streaming) |
| Screenshot caption (s05) | under user bubble, right-aligned, 13 `#9c9da1`, "Sent with screenshot" + image icon 16 |
| User bubble (all) | bg `#04398f` (s07) / `#043b93` (s04b), radius full (≥17), padding 8×16, text 15 white, single-line height 37, right-aligned with 18 inset |

| State | Spec |
|---|---|
| streaming | text appears progressively, last line at reduced opacity (measured) |
| complete | full opacity, fade mask removed (estimate) |
| error | not observed — estimate: danger color `#ef4444` label row |

## Meeting / activity row (s01)

| Part | Measured spec |
|---|---|
| Row | pitch 44 (text band pitch 88 raw), no divider, bg = page `#fafafa`; hover: estimate bg `#f4f4f5` |
| Title | 14/500 `#1a1a1a`, left at content column (x≈280 logical) |
| Duration chip | 38×19, radius full, bg `#f1f1f2`, text 11 `#3f3f46` (estimate) |
| Uses chip | ~44×19, radius full, bg `#ddedf9`, text 11 blue `#2f80d8` (estimate) |
| Timestamp | 11–12 `#8a8a8e` (estimate), right of chips, fixed column |
| Calendar badge | 14 blue outline icon after title when meeting-linked |
| Section label | "Today"/"Yesterday" 12/500 `#8a8a8e`, 24 above first row (estimate) |

States: default (measured); hover = estimate; selected/active not observed — estimate bg `#eef4ff` + 3px left accent bar `#38a3ee` (accent borrowed from dashboard azure).

## Tabs — Summary / Transcript (s02: Summary · Transcript · Usage)

| Part | Measured spec |
|---|---|
| Track | 205×31, radius full, bg `#f4f4f5`, padding 3 |
| Active tab | white pill `#ffffff`, radius full, shadow `0 1px 3px rgba(0,0,0,0.08)` (estimate), text 12–13/600 `#18181b` |
| Inactive tab | text 12–13/500 `#71717a` (estimate), transparent bg |
| Widget variant (s04b/s05/s07) | `Chat`/`Transcript` as nav-bar pills (see nav bar) — same token relationship, dark context |

States: default/active measured; hover estimate bg `rgba(0,0,0,0.04)`; disabled estimate opacity 0.4.

## Toggle switch (s01 "Detectable")

| State | Measured / estimate |
|---|---|
| size | 35×19, radius full, knob 15 white |
| OFF (measured) | track `#e4e4e7`, knob left, shadow `0 1px 2px rgba(0,0,0,0.10)` (estimate) |
| ON (not observed) | estimate: track `#38a3ee` (dashboard azure), knob right |
| hover / disabled | estimate: track darkens 8% / opacity 0.4 |

Pill-toggles (s07) are the widget's equivalent: OFF = dark pill `#1e1e24` + border; ON = tinted fill (`#1a2e4c` blue / `#c8b850` gold).

## Settings row (no settings screen in refs — estimate from dropdown/menu rows, s01)

| Part | Spec |
|---|---|
| Row | height 44–48, padding-x 16, label 14 `#1a1a1a`, optional value/chevron right 13 `#8a8a8e` |
| Dropdown menu (measured) | width 215, radius 12, bg `#ffffff`, shadow `0 12px 32px rgba(0,0,0,0.12)` (estimate), item pitch 38, checkmark `#38a3ee`, divider before primary action |
| Primary action row ("Manage Modes") | full-width, bg `#38a3ee`, text 14/600 white, bottom radius 12 (measured) |
| states | hover estimate bg `#f4f4f5`; active = azure fill (measured); disabled estimate opacity 0.4 |

## Notification popup (not present in any ref — estimate)

| Part | Spec |
|---|---|
| shape | floating card 320×64, radius 12, light: bg `#ffffff` border `#e3e3e6` shadow `0 12px 32px rgba(0,0,0,0.12)`; dark/glass: `rgba(33,33,37,0.75)` + blur 24 + border `rgba(255,255,255,0.08)` |
| content | icon 20 + title 14/600 + caption 12 secondary, gap 12, padding 16 |
| motion | enter 180ms slide-up 8px + fade; exit 120ms (durations default — not observable in stills) |

## Floating toolbar (s04/s04b/s05/s07) — context for popup-style chrome

| Variant | Measured |
|---|---|
| compact Ask (s04) | pill 165×50, radius full, bg `rgba(47,48,52,0.90)`, logo 24, blue `Ask` sub-pill `#03388c` white 14/600, stop square button 30 |
| standard (s04b/s05) | pill 213×46 (s05: 195×40), radius full, bg `rgba(47,48,52,0.90)`; segmented pause‖/stop group bg `#1c1c20`, divider 1px; chevron/grid icons 18–20 `#c8c8cd`; gap 8; separate close circle 36 (gap 8) |
| with mode selector (s07) | pill 346×55; inner mode pill ~176×37, border 1.5 `#57575b`, text 15 white + chevron; then pause/stop group, separator, grid; close circle 36 |
| states | hover estimate bg lightens to `#3a3a40`; pressed `#1c1c20`; disabled opacity 0.4 |
