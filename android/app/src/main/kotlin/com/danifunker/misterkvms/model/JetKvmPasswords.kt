package com.danifunker.misterkvms.model

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import com.danifunker.misterkvms.core.HostProfile
import com.danifunker.misterkvms.core.setJetkvmPassword
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * JetKVM passwords, one per JetKVM address: encrypted with an AES key that
 * lives in the Android Keystore and never leaves it, and kept in the app's
 * own storage, which isn't backed up or moved to another phone (the
 * manifest's data extraction rules), as DeviceKeystore keeps device keys.
 * The core never stores them: it gets them in memory with
 * `setJetkvmPassword` before it opens a JetKVM (docs/JETKVM.md).
 */
class JetKvmPasswords(context: Context) {
    private val prefs = context.getSharedPreferences("jetkvm-passwords", Context.MODE_PRIVATE)

    fun load(address: String): String = runCatching {
        val sealed = Base64.decode(prefs.getString(key(address), null) ?: return "", Base64.NO_WRAP)
        val cipher = Cipher.getInstance(TRANSFORMATION).apply {
            init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(128, sealed, 0, IV_SIZE))
        }
        String(cipher.doFinal(sealed, IV_SIZE, sealed.size - IV_SIZE))
    }.getOrDefault("")

    /** Save (or, empty, delete) the password and hand it to the core. */
    fun save(address: String, password: String) {
        if (password.isEmpty()) {
            prefs.edit().remove(key(address)).apply()
        } else {
            runCatching {
                val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, wrappingKey()) }
                val sealed = cipher.iv + cipher.doFinal(password.toByteArray())
                prefs.edit().putString(key(address), Base64.encodeToString(sealed, Base64.NO_WRAP)).commit()
            }
        }
        setJetkvmPassword(key(address), password)
    }

    /** At start-up and when hosts change: the core gets every JetKVM
     *  password the hosts need. */
    fun handTo(profiles: List<HostProfile>) {
        profiles.mapNotNull { it.jetkvmAddress }.toSet().forEach { setJetkvmPassword(it, load(it)) }
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

    companion object {
        private const val KEYSTORE = "AndroidKeyStore"
        private const val ALIAS = "misterkvms.jetkvm-passwords"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val IV_SIZE = 12

        /** The key a JetKVM is filed under: its address, as the core keys it. */
        fun key(address: String): String {
            var a = address.trim()
            for (p in listOf("jetkvm://", "http://")) if (a.startsWith(p, ignoreCase = true)) a = a.substring(p.length)
            return a.trimEnd('/').lowercase()
        }
    }
}

/** The JetKVM this host's video comes from directly (`jetkvm://…`), if any. */
val HostProfile.jetkvmAddress: String?
    get() = if (source == "local" && localVideo.startsWith("jetkvm://")) JetKvmPasswords.key(localVideo).ifEmpty { null } else null
