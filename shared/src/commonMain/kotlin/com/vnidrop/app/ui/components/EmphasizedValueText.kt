package com.vnidrop.app.ui.components

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight

internal fun emphasizedValueText(text: String, value: String): AnnotatedString {
	val start = text.indexOf(value)
	if (start < 0 || value.isEmpty()) return AnnotatedString(text)
	return buildAnnotatedString {
		append(text)
		addStyle(SpanStyle(fontWeight = FontWeight.Bold), start, start + value.length)
	}
}
