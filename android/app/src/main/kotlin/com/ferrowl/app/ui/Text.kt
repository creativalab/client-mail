package com.ferrowl.app.ui

/**
 * Fills `{name}` placeholders, the format used by the shared translations
 * (core/locales), e.g. `fill(stringResource(R.string.keep_days), "n" to 7)`.
 */
fun fill(template: String, vararg args: Pair<String, Any>): String =
    args.fold(template) { text, (name, value) -> text.replace("{$name}", value.toString()) }
