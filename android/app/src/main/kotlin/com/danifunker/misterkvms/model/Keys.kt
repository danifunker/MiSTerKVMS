package com.danifunker.misterkvms.model

/** USB HID keyboard usages (page 7) for the on-screen keys. */
object HID {
    const val ENTER: UShort = 0x28u
    const val ESCAPE: UShort = 0x29u
    const val BACKSPACE: UShort = 0x2Au
    const val TAB: UShort = 0x2Bu
    const val F1: UShort = 0x3Au
    const val F12: UShort = 0x45u
    const val DELETE: UShort = 0x4Cu
    const val RIGHT: UShort = 0x4Fu
    const val LEFT: UShort = 0x50u
    const val DOWN: UShort = 0x51u
    const val UP: UShort = 0x52u
    const val LEFT_CTRL: UShort = 0xE0u
    const val LEFT_SHIFT: UShort = 0xE1u
    const val LEFT_ALT: UShort = 0xE2u
    const val LEFT_GUI: UShort = 0xE3u

    /** F1…F12. */
    val functionKeys: List<Pair<String, UShort>> = (0 until 12).map { "F${it + 1}" to (F1 + it.toUShort()).toUShort() }
}

/** What's remembered for the app; the same keys as the iPhone app. What's
 *  shown on the session screen (P14) is remembered for the app, not per host. */
object Pref {
    const val KEYBOARD = "show.keyboard"
    const val FUNCTION_BAR = "show.functionBar"
    const val DPAD = "show.dpad"
    const val F12_BUTTON = "show.f12"
    const val BACKSPACE_BUTTON = "show.backspace"
    const val BANNER = "show.banner"
    const val STATS = "show.stats"
    /** How opaque the quick buttons are (0.25–1). */
    const val BUTTON_OPACITY = "quick.opacity"
    /** The quick buttons folded into one small button. */
    const val QUICK_COLLAPSED = "quick.collapsed"
    /** The Demo host is in the list (docs/APP-STORE.md S1). */
    const val DEMO_MODE = "demo.mode"
    /** The first-run screen has been answered. */
    const val WELCOMED = "welcome.done"
}
