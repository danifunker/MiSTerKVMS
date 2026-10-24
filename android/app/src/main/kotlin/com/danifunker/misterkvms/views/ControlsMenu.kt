package com.danifunker.misterkvms.views

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.Pref
import com.danifunker.misterkvms.model.subtitle

/** Show or hide each part of the session screen (P14); back to the host list. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ControlsMenu(model: AppModel, onDismiss: () -> Unit, arrange: () -> Unit) {
    var typing by rememberSaveable { mutableStateOf(false) }
    ModalBottomSheet(onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = typing)) {
        if (typing) {
            TypeTextScreen(model, back = { typing = false }, done = onDismiss)
            return@ModalBottomSheet
        }
        val showDpad = model.prefs.bool(Pref.DPAD, true)
        val showF12 = model.prefs.bool(Pref.F12_BUTTON, true)
        val showBackspace = model.prefs.bool(Pref.BACKSPACE_BUTTON, true)
        var opacity by model.prefs.float(Pref.BUTTON_OPACITY, 0.6f)
        Column(Modifier.verticalScroll(rememberScrollState()).navigationBarsPadding().padding(bottom = 16.dp)) {
            SheetTitle(model.profile?.name ?: "MiSTer", onDismiss)
            Section("Show")
            Toggle("Keyboard", model.prefs.bool(Pref.KEYBOARD, true))
            Toggle("Function keys", model.prefs.bool(Pref.FUNCTION_BAR, true))
            Toggle("Control banner", model.prefs.bool(Pref.BANNER, true))
            Toggle("Stats", model.prefs.bool(Pref.STATS, false))
            Toggle("Sound", !model.muted) { model.muted = !it }

            Toggle("Predictive cursor", model.predictiveCursor) { model.predictiveCursor = it }
            Footer(
                if (model.state.stream?.internet == true) {
                    "Draws a pointer ahead of the MiSTer's own to hide the delay. Over the internet the delay " +
                        "jumps around, so it's off unless you turn it on; home and the internet each remember " +
                        "their own setting."
                } else {
                    "Draws a pointer ahead of the MiSTer's own to hide the delay. Home and the internet each " +
                        "remember their own setting."
                },
            )

            Section("Quick buttons")
            Toggle("Arrows and OK", showDpad)
            Toggle("F12 (MiSTer menu)", showF12)
            Toggle("Backspace", showBackspace)
            if (showDpad.value || showF12.value || showBackspace.value) {
                Row(Modifier.padding(horizontal = 24.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("See-through", Modifier.padding(end = 16.dp))
                    Slider(opacity, { opacity = it }, valueRange = 0.25f..1f)
                }
                Action("Move buttons…") {
                    onDismiss()
                    arrange()
                }
                Action("Reset positions") { resetQuickButtonPositions(model) }
            }

            HorizontalDivider(Modifier.padding(vertical = 8.dp))
            Action("Type text…", enabled = model.isConnected) { typing = true }
            Action("Reset zoom") { model.resetZoom() }
            if (model.someoneElseInControl) {
                Action("Take control") {
                    model.takeControl()
                    onDismiss()
                }
            }
            Footer("Three-finger tap: this menu, even with everything hidden. Three-finger swipe: next or previous MiSTer.")
            HorizontalDivider(Modifier.padding(vertical = 8.dp))
            Action("Disconnect", destructive = true) {
                onDismiss()
                model.close()
            }
        }
    }
}

/** Every saved MiSTer, the open one ticked: tap another to switch to it, or
 *  disconnect and go back to the list. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MiSTerSwitcher(model: AppModel, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismiss) {
        Column(Modifier.verticalScroll(rememberScrollState()).navigationBarsPadding().padding(bottom = 16.dp)) {
            SheetTitle("MiSTers", onDismiss)
            model.profiles.forEach { profile ->
                Row(
                    Modifier.fillMaxWidth().clickable {
                        onDismiss()
                        model.switchTo(profile)
                    }.padding(horizontal = 24.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(profile.name)
                        Text(profile.subtitle, style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    if (profile.id == model.state.profileId) {
                        Icon(Icons.Filled.Check, "Open", tint = MaterialTheme.colorScheme.primary)
                    }
                }
            }
            Footer("Three-finger swipe left or right also switches.")
            HorizontalDivider(Modifier.padding(vertical = 8.dp))
            Action("Disconnect", destructive = true) {
                onDismiss()
                model.close()
            }
        }
    }
}

@Composable
fun SheetTitle(title: String, onDone: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 24.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
        TextButton(onDone) { Text("Done") }
    }
}

@Composable
private fun Section(title: String) {
    Text(title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 24.dp, top = 16.dp, bottom = 4.dp))
}

@Composable
private fun Footer(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = 24.dp, vertical = 4.dp))
}

@Composable
private fun Toggle(title: String, state: MutableState<Boolean>) = Toggle(title, state.value) { state.value = it }

@Composable
private fun Toggle(title: String, on: Boolean, set: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable { set(!on) }.padding(horizontal = 24.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(title)
        Switch(on, set)
    }
}

@Composable
private fun Action(title: String, enabled: Boolean = true, destructive: Boolean = false, onClick: () -> Unit) {
    TextButton(onClick, Modifier.padding(horizontal = 12.dp), enabled = enabled) {
        Text(title, fontWeight = FontWeight.Medium,
            color = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
    }
}
