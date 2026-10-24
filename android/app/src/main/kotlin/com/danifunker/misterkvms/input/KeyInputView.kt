package com.danifunker.misterkvms.input

import android.content.Context
import android.text.InputType
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager

/**
 * Receives the soft keyboard (P10). It sends characters, not keys: the core
 * types them as key presses on a US layout. Backspace and Enter are keys.
 * Nothing is kept here: the keyboard always sees an empty field, so
 * Backspace reaches us even with nothing typed.
 */
class KeyInputView(context: Context) : View(context) {
    var onText: (String) -> Unit = {}
    var onBackspace: () -> Unit = {}
    var onEnter: () -> Unit = {}

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onCheckIsTextEditor() = true

    override fun onCreateInputConnection(info: EditorInfo): InputConnection {
        // Plain keys: no suggestions, corrections, capitals or composing
        // (a visible-password field gets none of those on every keyboard).
        info.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        info.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN or
            EditorInfo.IME_ACTION_NONE
        return Connection()
    }

    fun showKeyboard() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java).showSoftInput(this, 0)
    }

    fun hideKeyboard() {
        context.getSystemService(InputMethodManager::class.java).hideSoftInputFromWindow(windowToken, 0)
    }

    private inner class Connection : BaseInputConnection(this, false) {
        override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
            send(text.toString())
            return true
        }

        // Some keyboards still compose: type it once it's committed.
        override fun setComposingText(text: CharSequence, newCursorPosition: Int) = true

        override fun finishComposingText() = true

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            repeat(beforeLength.coerceAtLeast(1)) { onBackspace() }
            return true
        }

        override fun sendKeyEvent(event: KeyEvent): Boolean {
            if (event.action != KeyEvent.ACTION_DOWN) return true
            when (event.keyCode) {
                KeyEvent.KEYCODE_DEL -> onBackspace()
                KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> onEnter()
                else -> event.unicodeChar.takeIf { it != 0 }?.let { send(it.toChar().toString()) }
            }
            return true
        }

        override fun performEditorAction(actionCode: Int): Boolean {
            onEnter()
            return true
        }

        /** Enter as the key, the rest as text. */
        private fun send(text: String) {
            text.split('\n').forEachIndexed { i, part ->
                if (i > 0) onEnter()
                if (part.isNotEmpty()) onText(part)
            }
        }
    }
}
