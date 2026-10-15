# Strings

`en.json` is the source locale, `ru.json` the second. Values use ICU MessageFormat.
Code refers to keys only; `strings.xml` (Android) and `.xcstrings` (iOS) are generated from these files and never edited by hand.
`tools/check-i18n.py` fails CI when a locale misses a key or has an extra one.
