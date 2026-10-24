package com.danifunker.misterkvms.views

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Backspace
import androidx.compose.material.icons.filled.CloseFullscreen
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.OpenWith
import androidx.compose.material.icons.outlined.Gamepad
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.HID
import com.danifunker.misterkvms.model.Pref
import kotlin.math.roundToInt

private val DPAD_SIZE = 168.dp
private val SMALL_SIZE = DpSize(56.dp, 44.dp)
private val EXPAND_SIZE = 48.dp

/** Where each piece starts, as fractions of the free space. */
private val starts = mapOf("dpad" to Offset(0.95f, 0.7f), "f12" to Offset(0.04f, 1f), "backspace" to Offset(0.96f, 1f))

/**
 * The quick buttons (P11): arrows around OK (Enter), F12 (the MiSTer menu)
 * and Backspace. Each piece is shown or hidden on its own from the controls
 * menu, and in Arrange mode each can be dragged anywhere on the screen; the
 * places are remembered as fractions of the space above the keyboard. The
 * D-pad also moves on its own: drag the handle on its top-left corner. The
 * button on its other corner collapses them all into one small round
 * button where the D-pad was; tapping that brings them back, dragging it
 * moves it (and the D-pad: they share their place).
 */
@Composable
fun QuickButtons(model: AppModel, arranging: Boolean, modifier: Modifier = Modifier) {
    val showDpad by model.prefs.bool(Pref.DPAD, true)
    val showF12 by model.prefs.bool(Pref.F12_BUTTON, true)
    val showBackspace by model.prefs.bool(Pref.BACKSPACE_BUTTON, true)
    val opacity by model.prefs.float(Pref.BUTTON_OPACITY, 0.6f)
    var collapsed by model.prefs.bool(Pref.QUICK_COLLAPSED, false)

    BoxWithConstraints(modifier.fillMaxSize()) {
        if (arranging) {
            // Swallows touches, so arranging never clicks on the MiSTer.
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.35f)).pointerInput(Unit) {
                detectDragGestures { _, _ -> }
            }.clickable(remember { MutableInteractionSource() }, indication = null) {})
        }
        val area = DpSize(maxWidth, maxHeight)
        if (collapsed && !arranging) {
            if (showDpad || showF12 || showBackspace) {
                Floating(model, "dpad", DpSize(EXPAND_SIZE, EXPAND_SIZE), area, arranging = false,
                    opacity = maxOf(opacity, 0.8f), draggable = true) {
                    RoundIcon(Icons.Outlined.Gamepad, "Show quick buttons", EXPAND_SIZE) { collapsed = false }
                }
            }
        } else {
            if (showDpad) {
                Floating(model, "dpad", DpSize(DPAD_SIZE, DPAD_SIZE), area, arranging, opacity, handle = true) {
                    DPad(model)
                    if (!arranging) {
                        Box(Modifier.align(Alignment.TopEnd)) {
                            RoundIcon(Icons.Filled.CloseFullscreen, "Collapse quick buttons", 30.dp) { collapsed = true }
                        }
                    }
                }
            }
            if (showF12) {
                Floating(model, "f12", SMALL_SIZE, area, arranging, opacity) {
                    SmallButton("F12", text = "F12") { model.openMenu() }
                }
            }
            if (showBackspace) {
                Floating(model, "backspace", SMALL_SIZE, area, arranging, opacity) {
                    SmallButton("Backspace", icon = Icons.AutoMirrored.Outlined.Backspace) { model.tap(HID.BACKSPACE) }
                }
            }
        }
    }
}

/** Forget where the buttons were moved to. */
fun resetQuickButtonPositions(model: AppModel) {
    for ((id, start) in starts) {
        model.prefs.float("quick.$id.x", start.x).value = start.x
        model.prefs.float("quick.$id.y", start.y).value = start.y
    }
}

/** One piece placed at a remembered spot; draggable in Arrange mode, so
 *  pressing a button never moves it, or by its handle if it has one. */
@Composable
private fun Floating(
    model: AppModel,
    id: String,
    size: DpSize,
    area: DpSize,
    arranging: Boolean,
    opacity: Float,
    handle: Boolean = false,
    draggable: Boolean = false,
    content: @Composable androidx.compose.foundation.layout.BoxScope.() -> Unit,
) {
    val start = starts.getValue(id)
    var x by model.prefs.float("quick.$id.x", start.x)
    var y by model.prefs.float("quick.$id.y", start.y)
    var drag by remember { mutableStateOf(Offset.Zero) }
    val density = LocalDensity.current
    val freeW = with(density) { (area.width - size.width).toPx() }.coerceAtLeast(0f)
    val freeH = with(density) { (area.height - size.height).toPx() }.coerceAtLeast(0f)
    val left = (freeW * x + drag.x).coerceIn(0f, freeW)
    val top = (freeH * y + drag.y).coerceIn(0f, freeH)

    // Drag to a new spot, remembered as fractions of the free space.
    val move = Modifier.pointerInput(freeW, freeH) {
        detectDragGestures(
            onDragEnd = {
                if (freeW > 0) x = ((freeW * x + drag.x) / freeW).coerceIn(0f, 1f)
                if (freeH > 0) y = ((freeH * y + drag.y) / freeH).coerceIn(0f, 1f)
                drag = Offset.Zero
            },
            onDragCancel = { drag = Offset.Zero },
        ) { change, amount ->
            change.consume()
            drag += amount
        }
    }

    Box(
        Modifier
            .offset { IntOffset(left.roundToInt(), top.roundToInt()) }
            .size(size)
            .then(if (draggable && !arranging) move else Modifier),
    ) {
        Box(Modifier.fillMaxSize().alpha(if (arranging) 1f else opacity), content = content)
        if (arranging) {
            // On top: in Arrange mode the piece is only moved, never pressed.
            Box(Modifier.fillMaxSize().border(2.dp, Color(0xFF0A84FF), RoundedCornerShape(14.dp)).then(move))
        }
        if (handle && !arranging) {
            Box(Modifier.align(Alignment.TopStart).then(move)) {
                RoundIcon(Icons.Filled.OpenWith, "Move the arrows", 30.dp, onClick = null)
            }
        }
    }
}

/** Arrows around OK (Enter). */
@Composable
private fun DPad(model: AppModel) {
    Box(
        Modifier.size(DPAD_SIZE).background(Color.Black.copy(alpha = 0.7f), CircleShape)
            .border(1.dp, Color.White.copy(alpha = 0.4f), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        val third = DPAD_SIZE / 3
        Arrow(Icons.Filled.KeyboardArrowUp, "Up", Modifier.offset(y = -third)) { model.tap(HID.UP) }
        Arrow(Icons.Filled.KeyboardArrowDown, "Down", Modifier.offset(y = third)) { model.tap(HID.DOWN) }
        Arrow(Icons.Filled.KeyboardArrowLeft, "Left", Modifier.offset(x = -third)) { model.tap(HID.LEFT) }
        Arrow(Icons.Filled.KeyboardArrowRight, "Right", Modifier.offset(x = third)) { model.tap(HID.RIGHT) }
        Box(
            Modifier.size(58.dp).background(Color.White.copy(alpha = 0.2f), CircleShape).clickable { model.tap(HID.ENTER) },
            contentAlignment = Alignment.Center,
        ) {
            Text("OK", color = Color.White, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        }
    }
}

@Composable
private fun Arrow(icon: ImageVector, label: String, modifier: Modifier, onClick: () -> Unit) {
    Box(modifier.size(52.dp).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
        Icon(icon, label, tint = Color.White, modifier = Modifier.size(30.dp))
    }
}

/** A single round-ended key: F12 or Backspace. */
@Composable
private fun SmallButton(label: String, text: String? = null, icon: ImageVector? = null, onClick: () -> Unit) {
    Box(
        Modifier.size(SMALL_SIZE).background(Color.Black.copy(alpha = 0.7f), CircleShape)
            .border(1.dp, Color.White.copy(alpha = 0.4f), CircleShape).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        if (text != null) Text(text, color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
        if (icon != null) Icon(icon, label, tint = Color.White, modifier = Modifier.size(20.dp))
    }
}

@Composable
private fun RoundIcon(icon: ImageVector, label: String, size: androidx.compose.ui.unit.Dp, onClick: (() -> Unit)?) {
    Box(
        Modifier.size(size).background(Color.Black.copy(alpha = 0.8f), CircleShape)
            .border(1.dp, Color.White.copy(alpha = 0.4f), CircleShape)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, label, tint = Color.White, modifier = Modifier.size(size * 0.45f))
    }
}
