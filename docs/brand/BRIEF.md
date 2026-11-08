# Ozero brand update: Ozero Mail

New identity for the Ozero family. Two tasks for this repo: replace the app icon and logo assets, and move the colour scheme to the new palette. Everything you need is in this folder.

## The idea

The mark is sky over water inside a circle. Every product has its own symmetric shoreline: Mail is an envelope flap (V), Phone is a bowl (U), the Ozero brand is a straight horizon. The app icon is the same picture filling the whole tile. The logo is always flat; glass and depth come only from the system in the app icon.

## 1. App icon

- **iOS.** `icons/ios/icon-composer/` holds the layers for Icon Composer: `0_water.svg` (background, vertical gradient) and `1_sky.svg` (one white shape). Build an `.icon` from them. In Icon Composer, set the sky fill to a gradient `#FFFFFF → #EEE8DD`, turn the sky layer's translucency off (or ≤10%), shadow Neutral ~40%. For Tinted, set the sky to white and the water to grey `#9A9A9A → #3C3C3C` by hand.
  Until then, `icons/ios/AppIcon-1024-{default,dark,tinted}.png` drop straight into `ios/OzeroMail/Assets.xcassets/AppIcon.appiconset` as `icon.png`, `icon-dark.png`, `icon-tinted.png`.
- **Android.** `icons/android/res/` replaces `ic_launcher_background.xml`, `ic_launcher_foreground.xml`, `ic_launcher_monochrome.xml` and `mipmap-anydpi-v26/ic_launcher.xml` (this repo keeps it in `mipmap-anydpi/`). `play-store-512.png` is the store icon.
- **In-app mark.** Replace `drawable/ozero_mark.xml` and `Assets.xcassets/Mark.imageset` with `logo/ozero-mail-mark.svg` (colour) or `-black` / `-white` (one colour).
- **Web.** `icons/web/` has `favicon.svg`, 16/32 px PNGs and `apple-touch-icon.png`.

Check the result on a real device in every mode: Default, Dark, Clear, Tinted on iOS; themed icons and a couple of launcher masks on Android.

## 2. Colour scheme

Old palette: a single bright blue `#2F8CFF` on cool greys. New palette: the lake's deep blue, warm sand, navy ink.

| Role | Light | Dark | Was (light / dark) |
|---|---|---|---|
| Fill (`accent`, `ozeroFill`): buttons, Done, marks; white on it 4.5:1 | `#2F6BFF` | `#2F6BFF` | `#2F8CFF` |
| Text accent (`primary`, `ozeroAccent`): 6.2:1 on white, 7.5:1 on night | `#2456D6` | `#7FA9FF` | `#1A6FE0` / `#6AB2FF` |
| Primary container | `#E9F0FF` | `#1B2D5C` | `#E6F1FF` / `#15325A` |
| Ink (`onSurface`, `ozeroInk`) | `#12192A` | `#E9EDF4` | `#142036` / `#E5E9F1` |
| Muted (`onSurfaceVariant`, `ozeroMuted`): 4.8:1 on white | `#6B7280` | `#94A3B8` | `#66728A` / `#8792A7` |
| Page (`background`, `ozeroPage`) | `#FFFFFF` | `#12192A` | `#FFFFFF` / `#141923` |
| Warm surface (cards, sheets) — new | `#F6F2EC` | `#1A2234` | — |
| Deep (brand, gradient bottoms) — new | `#1B3F7A` | `#1B3F7A` | — |

Keep error, snooze (the late sun) and the neutral greys as they are; nudge only the greys that sit next to the new warm surface if they clash. Update `Theme.kt` (both schemes, `Accent`), `Theme.swift` (`ozeroAccent`, `ozeroFill`, `ozeroInk`, `ozeroMuted`, `ozeroPage`, `ozeroCard`) and `values/colors.xml` (`window_night`) together, so Android and iOS stay in step. Re-check contrast for small text after the change.

## 3. Logo and name

- Lockups in outlines (no font needed): `logo/ozero-mail-lockup-{light,dark,black,white}.svg`; Russian: `ozero-mail-ru-lockup-*` reads «озеро почта».
- The wordmark is lowercase only in the logo; in text the name stays "Ozero Mail".
- Mark is 0.78 of the word's size, centred on the x-height, 0.3 of the size away from the word. Minimum: 16 px for the mark, 16 px type for the lockup with "mail". Keep clear space of one x-height around it.
- Don't stretch, add shadows or glow, recolour, set in capitals, or put the colour lockup on a busy background.

Wordmark type: Manrope SemiBold (logo only). UI type is unchanged.
