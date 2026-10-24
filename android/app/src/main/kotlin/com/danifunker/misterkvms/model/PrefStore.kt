package com.danifunker.misterkvms.model

import android.content.Context
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf

/** The app's settings (SharedPreferences) as Compose state: every screen
 *  reading a key sees the same value, and a change is saved at once. */
class PrefStore(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
    private val bools = HashMap<String, MutableState<Boolean>>()
    private val floats = HashMap<String, MutableState<Float>>()

    fun bool(key: String, default: Boolean): MutableState<Boolean> =
        bools.getOrPut(key) { Saved(mutableStateOf(prefs.getBoolean(key, default))) { prefs.edit().putBoolean(key, it).apply() } }

    fun float(key: String, default: Float): MutableState<Float> =
        floats.getOrPut(key) { Saved(mutableStateOf(prefs.getFloat(key, default))) { prefs.edit().putFloat(key, it).apply() } }

    /** A state whose every change is also written to the preferences. */
    private class Saved<T>(private val state: MutableState<T>, private val save: (T) -> Unit) : MutableState<T> by state {
        override var value: T
            get() = state.value
            set(v) {
                state.value = v
                save(v)
            }

        override fun component2(): (T) -> Unit = { value = it }
    }
}
