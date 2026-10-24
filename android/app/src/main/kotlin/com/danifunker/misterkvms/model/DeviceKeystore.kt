package com.danifunker.misterkvms.model

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONObject
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Device keys for connecting from the internet (docs/IPHONE.md §2), one per
 * capture PC (filed under the name `Controller.deviceKeyServer` gives: the
 * PC's id). Each is encrypted with an AES key that lives in the Android
 * Keystore and never leaves it, and kept in the app's own storage, which
 * isn't backed up or moved to another phone (the manifest). The core gets
 * them at launch; they never go into its settings file. The iPhone app's
 * DeviceKeychain.
 */
class DeviceKeystore(context: Context) {
    data class Entry(val keyId: String, val key: String)

    private val prefs = context.getSharedPreferences("device-keys", Context.MODE_PRIVATE)

    fun save(entry: Entry, account: String): Boolean = runCatching {
        val json = JSONObject().put("keyId", entry.keyId).put("key", entry.key).toString()
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, wrappingKey()) }
        val sealed = cipher.iv + cipher.doFinal(json.toByteArray())
        prefs.edit().putString(account, Base64.encodeToString(sealed, Base64.NO_WRAP)).commit()
    }.getOrDefault(false)

    fun load(account: String): Entry? = runCatching {
        val sealed = Base64.decode(prefs.getString(account, null) ?: return null, Base64.NO_WRAP)
        val cipher = Cipher.getInstance(TRANSFORMATION).apply {
            init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(128, sealed, 0, IV_SIZE))
        }
        val json = JSONObject(String(cipher.doFinal(sealed, IV_SIZE, sealed.size - IV_SIZE)))
        Entry(json.getString("keyId"), json.getString("key"))
    }.getOrNull()

    fun delete(account: String) {
        prefs.edit().remove(account).apply()
    }

    /** The AES key in the Keystore, made the first time it's needed. */
    private fun wrappingKey(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE).apply { init(spec) }.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "misterkvms.device-keys"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        /** GCM's IV, which the Keystore picks for each encryption. */
        const val IV_SIZE = 12
    }
}
