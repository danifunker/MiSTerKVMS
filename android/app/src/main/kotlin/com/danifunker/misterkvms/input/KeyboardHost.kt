package com.danifunker.misterkvms.input

import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.viewinterop.AndroidView
import android.view.inputmethod.InputMethodManager
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.HID

/** The soft keyboard (P10), up while `showKeyboard` is on and nothing
 *  covers the session. Hiding it with Back turns the switch off, as the
 *  controls menu would; the menu brings it back. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun KeyboardHost(model: AppModel, showKeyboard: MutableState<Boolean>, covered: Boolean) {
    val want = showKeyboard.value && !covered
    var view by remember { mutableStateOf<KeyInputView?>(null) }
    AndroidView(
        factory = { context ->
            KeyInputView(context).apply {
                onText = { model.type(it) }
                onBackspace = { model.tap(HID.BACKSPACE) }
                onEnter = { model.tap(HID.ENTER) }
                view = this
            }
        },
        modifier = Modifier.size(1.dp),
    )
    LaunchedEffect(want, view) {
        val v = view ?: return@LaunchedEffect
        // Once the view is in a window that has focus, or the keyboard won't come up.
        v.post { if (want) v.showKeyboard() else v.hideKeyboard() }
    }
    // Leaving the session takes the keyboard with it (through the screen's
    // own view: the input view may have left the window by now).
    val host = LocalView.current
    DisposableEffect(host) {
        onDispose {
            host.context.getSystemService(InputMethodManager::class.java).hideSoftInputFromWindow(host.windowToken, 0)
        }
    }
    val imeVisible = WindowInsets.isImeVisible
    var wasVisible by remember { mutableStateOf(false) }
    LaunchedEffect(imeVisible) {
        if (wasVisible && !imeVisible && want) showKeyboard.value = false
        wasVisible = imeVisible
    }
}

/** Keys the soft keyboard doesn't have: Esc, Tab, modifiers (tap: next key
 *  only; tap again: locked; again: off), F1–F12, Del. */
@Composable
fun FunctionBar(model: AppModel, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .height(46.dp)
            .background(Color(0xE61C1C1E))
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Key("Esc") { model.tap(HID.ESCAPE) }
        Key("Tab") { model.tap(HID.TAB) }
        ModifierKey("Ctrl", HID.LEFT_CTRL, model)
        ModifierKey("Alt", HID.LEFT_ALT, model)
        ModifierKey("Shift", HID.LEFT_SHIFT, model)
        ModifierKey("Win", HID.LEFT_GUI, model)
        VerticalDivider(Modifier.height(24.dp))
        HID.functionKeys.forEach { (name, usage) -> Key(name) { model.tap(usage) } }
        VerticalDivider(Modifier.height(24.dp))
        Key("Del") { model.tap(HID.DELETE) }
    }
}

@Composable
private fun Key(title: String, active: Boolean = false, locked: Boolean = false, onClick: () -> Unit) {
    Box(
        Modifier
            .widthIn(min = 46.dp)
            .background(
                if (active) MaterialTheme.colorScheme.primary else Color.White.copy(alpha = 0.15f),
                RoundedCornerShape(6.dp),
            )
            .clickable(onClick = onClick)
            .padding(horizontal = 6.dp, vertical = 7.dp),
        contentAlignment = Alignment.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(title, color = if (active) Color.Black else Color.White, fontSize = 15.sp,
                fontWeight = FontWeight.Medium, textAlign = TextAlign.Center)
            if (locked) Icon(Icons.Filled.Lock, "Locked", tint = Color.Black, modifier = Modifier.size(10.dp))
        }
    }
}

@Composable
private fun ModifierKey(title: String, usage: UShort, model: AppModel) {
    val state = model.modifierState(usage)
    Key(title, active = state != AppModel.ModifierState.OFF, locked = state == AppModel.ModifierState.LOCKED) {
        model.cycleModifier(usage)
    }
}
