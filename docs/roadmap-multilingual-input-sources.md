# Slipkey Phase 2: Multilingual Input Source Detection

Status: planned, not implemented.

This document captures the next-stage product requirement for Slipkey after the initial English / Japanese / Chinese workflow.

## Goal

Slipkey should adapt to the input methods installed on the user's current computer.

On a fresh machine, the app should:

1. Detect installed keyboard layouts and input methods.
2. Normalize each detected source to an international language code.
3. Generate sensible default typed shortcuts from those language codes.
4. Render the frontend language list from detected languages instead of hardcoded `en`, `ja`, and `zh`.
5. Preserve user choices when input sources are re-detected.

Examples:

```text
English   -> en -> ;en
Japanese  -> ja -> ;ja
Chinese   -> zh -> ;zh
Korean    -> ko -> ;ko
French    -> fr -> ;fr
German    -> de -> ;de
Spanish   -> es -> ;es
```

## Product Model

The UI should show one row per language, not one row per input source.

If a language has multiple available sources, the source cell should be a dropdown. This matches the current macOS frontend model and keeps the settings page compact.

Example:

```text
Enabled | Language | Prefix | Source
------------------------------------------------------
   ✓    | English  | en     | [ABC                  v]
   ✓    | Japanese | ja     | [Japanese - Romaji    v]
   ✓    | Chinese  | zh     | [Microsoft Pinyin     v]
   ✓    | Korean   | ko     | [Korean               v]
   ✓    | French   | fr     | [French               v]
```

If Chinese has multiple input sources, it should still be one row:

```text
Chinese | zh | [Microsoft Pinyin v]
              [Wubi]
              [Shuangpin]
```

## Data Model

The detected source model should keep all platform detail, while the mapping model should represent the user's selected source for each language.

Suggested detected source shape:

```text
DetectedInputSource
  platform
  source_id
  display_name
  raw_language_tag
  normalized_language
  selectable
```

Suggested config shape:

```toml
leader = ";"

[[mappings]]
language = "ja"
prefix = "ja"
source = "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese"
name = "Japanese - Romaji"
enabled = true
```

Notes:

- `language` should be a normalized language code such as `en`, `ja`, `zh`, `ko`, `fr`, or `de`.
- `prefix` should default to the language code.
- `source` should store the selected platform-native source ID.
- `name` is display metadata and can be refreshed from detection.
- `enabled` controls whether the row participates in trigger matching.

For backward compatibility, older configs without `name` or `enabled` should continue to load.

## Language Normalization

Normalize platform language tags to short standard language codes:

```text
en-US       -> en
en          -> en
ja-JP       -> ja
ja          -> ja
zh-Hans-CN  -> zh
zh-Hant-TW  -> zh
ko-KR       -> ko
fr-FR       -> fr
de-DE       -> de
es-ES       -> es
```

When a platform provides only a Windows LANGID, map it to an ISO-style code:

```text
0x0409 -> en
0x0411 -> ja
0x0804 -> zh
0x0404 -> zh
0x0C04 -> zh
0x1404 -> zh
0x0412 -> ko
0x040C -> fr
0x0407 -> de
0x0C0A -> es
```

## Merge Rules

Detection should be repeatable and should not erase user preferences.

Rules:

1. Group detected sources by normalized language.
2. Render one mapping row per language.
3. If an existing mapping's selected source still exists, keep it.
4. If the selected source disappeared, keep the row and mark it unavailable in UI.
5. If a language is newly detected, create a row with:
   - `language = normalized_language`
   - `prefix = normalized_language`
   - `source = preferred source for that language`
   - `enabled = true`
6. If a language has multiple sources, choose a default source but expose all candidates in the dropdown.
7. Preserve custom prefixes across detection.
8. Validate that prefixes are unique and no prefix starts with another configured prefix.

## Source Selection Priority

When a language has multiple sources, choose a sensible default.

Initial priority can be simple:

1. Previously selected source.
2. Source whose display name matches common defaults.
3. First selectable source returned by the platform.

Possible preferred names:

```text
English:  ABC, US, English
Japanese: Japanese - Romaji, Microsoft Japanese IME
Chinese:  Microsoft Pinyin, Pinyin, Shuangpin
Korean:   Korean
French:   French
German:   German
Spanish:  Spanish
```

This priority can be improved later with locale-specific rules.

## macOS Plan

Current macOS detection is close to the desired model because TIS input sources expose stable source IDs.

Required changes:

1. Update `InputSourceService` so it no longer filters to only `en`, `ja`, and `zh`.
2. Normalize any supported BCP-47 language tag to a short language code.
3. Keep all selectable sources grouped by language.
4. Change config merging so it creates one row per language, with a selected source from the grouped candidates.
5. Keep source selection in the frontend as a dropdown.
6. Preserve the user's selected source when Detect is clicked again.
7. Show unavailable selected sources clearly if the source is no longer installed.

macOS switching can continue to use the selected source ID directly.

## Windows Plan

Windows detection needs more work because HKL and TSF profiles are not the same abstraction.

Required changes:

1. Keep current HKL detection as the baseline.
2. Expand language recognition beyond `en`, `ja`, `zh`, and `ko`.
3. Add a TSF profile enumeration layer if HKL detection is insufficient for modern IMEs.
4. Merge HKL / TSF profile results into a unified `DetectedInputSource` list.
5. Group sources by normalized language.
6. Render one row per language in the Windows settings UI.
7. Use a source dropdown for languages with multiple detected sources.
8. Preserve user-selected source IDs across detection.

Switching behavior should remain mode-aware:

- English / alphanumeric entries can use mode-only switching when appropriate.
- CJK languages should continue to use HKL switch plus TSF compartment write.
- Non-CJK layouts can use layout-only switching.
- Unknown languages should default to layout-only switching.

The switching mode should eventually be derived from detected source metadata, not only from the language string.

## Frontend Requirements

The settings UI should not hardcode a fixed list of languages.

It should render from config plus detected source groups:

```text
Language row
  enabled toggle
  localized display name
  editable prefix
  source dropdown
  status: ready / unavailable / duplicate prefix
```

Expected states:

- Ready: selected source is installed and selectable.
- Unavailable: selected source is missing from current detection.
- Conflict: prefix validation failed.
- New: language detected for the first time.

## Migration

Existing three-language configs must keep working.

Migration behavior:

1. Read old config.
2. Detect current sources.
3. Match old `language` and `source` values to detected candidates.
4. Preserve custom prefixes.
5. Add missing detected languages as new rows.
6. Save in the new schema only after the user saves settings or after a deliberate migration step.

Do not delete unknown or unavailable sources automatically.

## Acceptance Criteria

macOS:

- Installing English, Japanese, Chinese, Korean, and French input sources produces five language rows.
- Multiple Chinese sources appear in the Chinese row's source dropdown.
- `;fr` can switch to French after detection and save.
- User-customized prefixes survive another Detect.

Windows:

- Installing English, Japanese, Chinese, Korean, and French input sources produces matching language rows where Windows exposes them.
- Multiple sources for the same language appear in one row's dropdown.
- CJK switching keeps existing TSF behavior.
- Non-CJK language rows use layout-only switching.
- Existing `en`, `ja`, and `zh` configs continue to work.

Shared:

- No language list is hardcoded in the frontend.
- Prefix validation works across all detected languages.
- Missing sources are visible and recoverable.
- Fresh-machine setup does not require hand-editing config files.

## Non-Goals For This Phase

- Perfect ranking for every language and third-party IME.
- Cloud sync of preferences.
- Signed Windows installer.
- Per-app input source rules.
- Advanced aliases such as `;jp` for Japanese unless the user configures them.
