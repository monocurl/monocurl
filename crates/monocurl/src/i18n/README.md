# app localization

`en.json` is the source catalog. A translated catalog must preserve every key and only change its values. The app uses English for any missing key, so a partially translated catalog is safe while it is being reviewed.

To publish a new language, translate the matching JSON file. It appears in Settings only after it contains at least one translated entry. Keep Monocurl language keywords, file extensions, and keyboard shortcuts unchanged unless the string itself explicitly needs a translated display label.
