package com.ozeromail.app

import androidx.compose.material3.Typography
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

/** Weights the app uses; a variable font draws each one exactly. */
object Weight {
    val Regular = FontWeight(400)
    val Medium = FontWeight(500)
    val Firm = FontWeight(550)
    val SemiBold = FontWeight(600)
    val Strong = FontWeight(650)
    val Bold = FontWeight(700)
}

@OptIn(ExperimentalTextApi::class)
private fun variable(font: Int, weights: List<FontWeight>, optical: TextUnit? = null) = FontFamily(
    weights.map { weight ->
        val axes = listOfNotNull(FontVariation.weight(weight.weight), optical?.let { FontVariation.opticalSizing(it) })
        Font(font, weight, variationSettings = FontVariation.Settings(*axes.toTypedArray()))
    },
)

private val UiWeights = with(Weight) { listOf(Regular, Medium, Firm, SemiBold, Strong, Bold) }

/** Inter for everything but what is read at length, at its text optical size. */
private val Inter = variable(R.font.inter, UiWeights)

/** Inter cut for large sizes: tighter, with finer details. */
private val InterDisplay = variable(R.font.inter, listOf(Weight.Strong, Weight.Bold), optical = 32.sp)

/** Literata for reading messages, at its default optical size, as in the web view. */
private val Literata = variable(R.font.literata, listOf(Weight.Regular, Weight.SemiBold, Weight.Bold))

private val Even = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None)

private fun ui(
    size: Float,
    line: Float,
    weight: FontWeight = Weight.Regular,
    tracking: Float = 0f,
    family: FontFamily = Inter,
    lineBreak: LineBreak = LineBreak.Unspecified,
) = TextStyle(
    fontFamily = family,
    fontSize = size.sp,
    lineHeight = line.sp,
    fontWeight = weight,
    letterSpacing = tracking.em,
    lineHeightStyle = Even,
    lineBreak = lineBreak,
)

/** Figures that line up, for times, dates and counts. */
private fun TextStyle.tabular() = copy(fontFeatureSettings = "tnum")

/**
 * The type scale, by role. Mail is read more than anything else, so reading sets the scale:
 * text people read at length is Literata, everything around it Inter.
 */
object Type {
    /** Screen titles: Inbox, a folder. */
    val Display = ui(32f, 38f, Weight.Bold, -0.022f, InterDisplay, LineBreak.Heading)

    /** An open message's subject, and the subject being written. */
    val Title = ui(24f, 30f, Weight.Strong, -0.017f, InterDisplay, LineBreak.Heading)

    /** Titles of sheets and notes. */
    val Heading = ui(19f, 25f, Weight.Strong, -0.01f, lineBreak = LineBreak.Heading)

    /** A message's text. */
    val Reading = ui(17.5f, 28f, family = Literata, lineBreak = LineBreak.Paragraph).copy(hyphens = Hyphens.Auto)

    /** Text being written: Inter at the size it will be read at; lines do not reflow while typing. */
    val Writing = ui(17f, 27f, lineBreak = LineBreak.Simple)

    /** Quoted text under a message, a step quieter. */
    val Quote = Reading.copy(fontSize = 15.5.sp, lineHeight = 24.sp)

    /** A sender in a list, the name on an open message, rows of menus and settings. */
    val Row = ui(16f, 21f, Weight.Medium, -0.006f)

    /** A subject in a list. */
    val Subject = ui(15f, 20f)

    /** Short text around the app: explanations, fields, buttons. */
    val Body = ui(15f, 22f)

    /** A preview in a list, secondary lines. */
    val Text = ui(14f, 20f)

    /** Times, who wrote to whom, captions. */
    val Caption = ui(13f, 18f).tabular()

    /** Under the icons of a bar. */
    val Label = ui(11.5f, 14f, Weight.Medium, 0.01f)

    /** Counts on a thread or a tab. */
    val Badge = ui(11f, 14f, Weight.SemiBold).tabular()

    /** Material components fall back to these. */
    val Material = Typography(
        displayLarge = Display, displayMedium = Display, displaySmall = Display,
        headlineLarge = Title, headlineMedium = Title, headlineSmall = Heading,
        titleLarge = Heading, titleMedium = Row, titleSmall = Subject,
        bodyLarge = Body, bodyMedium = Text, bodySmall = Caption,
        labelLarge = Text.copy(fontWeight = Weight.SemiBold), labelMedium = Label, labelSmall = Badge,
    )
}
