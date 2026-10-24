package com.danifunker.misterkvms.views

import android.content.ClipboardManager
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.core.textTyping
import com.danifunker.misterkvms.core.typeSpeedRange
import com.danifunker.misterkvms.model.AppModel
import kotlin.math.ceil
import kotlin.math.roundToInt

/** Controls menu ▸ Type text…: text typed on the MiSTer key by key, as on a
 *  US keyboard (the MiSTer has no clipboard). The speed is for the core:
 *  some take keys slowly; it's remembered for the host. */
@Composable
fun TypeTextScreen(model: AppModel, back: () -> Unit, done: () -> Unit) {
    val range = remember { typeSpeedRange() }
    var text by rememberSaveable { mutableStateOf("") }
    var speed by rememberSaveable {
        val saved = model.profile?.typeSpeed ?: 0.0
        mutableFloatStateOf((if (saved > 0) saved.roundToInt().toDouble() else range[2]).toFloat())
    }
    val context = LocalContext.current
    val canType = model.isConnected && !model.someoneElseInControl && !model.watching
    val preview = textTyping(text, speed.toDouble())

    Column(Modifier.verticalScroll(rememberScrollState()).imePadding().navigationBarsPadding().padding(bottom = 16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 8.dp)) {
            IconButton(back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") }
            Text("Type text", style = MaterialTheme.typography.titleLarge)
        }
        OutlinedTextField(
            text, { text = it },
            Modifier.fillMaxWidth().heightIn(min = 140.dp).padding(horizontal = 24.dp),
            textStyle = TextStyle(fontFamily = FontFamily.Monospace),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
        )
        OutlinedButton(
            {
                val clip = context.getSystemService(ClipboardManager::class.java).primaryClip
                clip?.getItemAt(0)?.coerceToText(context)?.let { text = it.toString() }
            },
            Modifier.padding(horizontal = 24.dp, vertical = 4.dp),
        ) { Text("Paste") }
        Note("Typed key by key, as on a US keyboard.")

        Text("Speed", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(start = 24.dp, top = 16.dp))
        Row(Modifier.padding(horizontal = 24.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Slow", style = MaterialTheme.typography.labelSmall)
            Slider(
                speed, { speed = it.roundToInt().toFloat() },
                Modifier.weight(1f).padding(horizontal = 8.dp),
                valueRange = range[0].toFloat()..range[1].toFloat(),
            )
            Text("Fast", style = MaterialTheme.typography.labelSmall)
        }
        Note("${speed.roundToInt()} keys per second. Missed letters? Slow it down for this MiSTer.")

        Button(
            {
                model.typeText(text, speed.toDouble())
                done()
            },
            Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
            enabled = preview.keys > 0u && canType,
        ) { Text("Type") }
        Note(
            when {
                !canType -> "Take control of this MiSTer first."
                preview.keys == 0u -> "Paste or type the text above."
                else -> {
                    val time = if (preview.seconds < 60) "${ceil(preview.seconds).toInt()} s"
                    else "${ceil(preview.seconds / 60).toInt()} min"
                    var line = "${preview.keys} keys, about $time."
                    if (preview.skipped > 0u) {
                        val s = if (preview.skipped == 1u) "" else "s"
                        line += " ${preview.skipped} character$s can't be typed on a US keyboard and will be skipped."
                    }
                    line
                }
            },
        )
    }
}
