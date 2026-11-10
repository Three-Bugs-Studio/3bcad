# Translating 3BCAD

3BCAD's interface is in English and Vietnamese (Tiếng Việt). Pick the language in **File → Settings →
General**. Adding another language takes one text file and a few lines of code.

## How it works

Every text the interface shows goes through `tr("English text")` (or `trf("Opened {}", &[&name])`
when it has values in it) in `crates/3bcad-app/src`. English is the key. Each language has a file in
`crates/3bcad-app/i18n/` with one pair of lines per text:

```text
en: Open…
vi: Mở…

en: Opened {}
vi: Đã mở {}
```

A text without a translation shows in English, so a partial translation is fine to ship.

## Rules

- Copy the `en:` line exactly, including `…` (one character), quotes and punctuation. It is the
  lookup key.
- Keep as many `{}` as the English has. Values fill them in order: reword the sentence rather than
  move them.
- Keep key names (Ctrl, Shift, Esc, F1), file formats (STEP, STL, 3MF), units and product names as
  they are.
- Keep it short: labels sit on ribbon buttons and in narrow panels.

## Adding a language

1. Copy `crates/3bcad-app/i18n/vi.txt` to `<code>.txt` (for example `de.txt`) and replace the `vi:`
   lines with your language, keeping the `vi:` prefix or using your own code. Only the text after the
   colon is read.
2. In `crates/3bcad-app/src/i18n.rs`, add the language to `Language`, `ALL`, `name()` (in the language
   itself), `id()` and `table()`, and to `current()` and `set()`'s numbering.
3. If your script is not Latin, Greek or Cyrillic, check that `system_font()` finds a font that has
   it, and add one to its list if not.
4. Run `cargo test -p threebcad-app i18n`. It checks that every translation keeps its `{}`.
5. Open a pull request. Screenshots of the main tabs in your language help review.

## Finding untranslated texts

Switch to your language and look for English. When a text is missing from the file, copy it from the
code; a new feature's texts are in its `tr(...)` and `trf(...)` calls.
