package io.github.hewel.jellypilot.bridge

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.github.hewel.jellypilot.ffi.CredentialStoreException
import io.github.hewel.jellypilot.ffi.SecureCredentialStore
import java.io.File
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Protected credential storage backed by Android Keystore.
 *
 * The serialized profile blob is AES-256-GCM encrypted under a
 * Keystore-resident key that never leaves the Android Keystore, and
 * written to app-private no-backup storage. Plaintext tokens never touch
 * disk or logs; the encrypted blob is excluded from backup independently
 * of the application's backup/transfer policy.
 *
 * The SDK calls [read]/[write]/[delete] on its credential worker thread, so
 * blocking file and Keystore I/O here is expected.
 */
class KeystoreCredentialStore(context: Context) : SecureCredentialStore {
  private val blobFile = File(context.noBackupFilesDir, CREDENTIAL_FILE)

  override fun read(): ByteArray? {
    val blob = try {
      if (!blobFile.isFile) return null
      blobFile.readBytes()
    } catch (error: IOException) {
      throw CredentialStoreException.Unavailable()
    }
    if (blob.size < GCM_IV_BYTES + GCM_TAG_BYTES) {
      throw CredentialStoreException.Unavailable()
    }
    return try {
      val cipher = Cipher.getInstance(TRANSFORMATION)
      cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, blob, 0, GCM_IV_BYTES))
      cipher.doFinal(blob, GCM_IV_BYTES, blob.size - GCM_IV_BYTES)
    } catch (error: GeneralSecurityException) {
      throw CredentialStoreException.Unavailable()
    }
  }

  override fun write(secret: ByteArray) {
    val encrypted = try {
      val cipher = Cipher.getInstance(TRANSFORMATION)
      cipher.init(Cipher.ENCRYPT_MODE, secretKey())
      cipher.iv + cipher.doFinal(secret)
    } catch (error: GeneralSecurityException) {
      throw CredentialStoreException.WriteFailed()
    }
    // Write-then-rename keeps the previous blob intact if the write fails.
    val temporary = File(blobFile.parentFile, "$CREDENTIAL_FILE.tmp")
    try {
      temporary.writeBytes(encrypted)
      if (!temporary.renameTo(blobFile)) {
        throw CredentialStoreException.WriteFailed()
      }
    } catch (error: IOException) {
      temporary.delete()
      throw CredentialStoreException.WriteFailed()
    }
  }

  override fun delete() {
    try {
      if (blobFile.exists() && !blobFile.delete()) {
        throw CredentialStoreException.WriteFailed()
      }
    } catch (error: SecurityException) {
      throw CredentialStoreException.WriteFailed()
    }
  }

  private fun secretKey(): SecretKey {
    val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
    (keyStore.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
    generator.init(
      KeyGenParameterSpec.Builder(
        KEY_ALIAS,
        KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
      )
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setKeySize(KEY_BITS)
        .setRandomizedEncryptionRequired(true)
        .build(),
    )
    return generator.generateKey()
  }

  private companion object {
    const val ANDROID_KEYSTORE = "AndroidKeyStore"
    const val KEY_ALIAS = "jellypilot-saved-profiles"
    const val CREDENTIAL_FILE = "saved-profiles.cred"
    const val TRANSFORMATION = "AES/GCM/NoPadding"
    const val KEY_BITS = 256
    const val GCM_IV_BYTES = 12
    const val GCM_TAG_BITS = 128
    const val GCM_TAG_BYTES = 16
  }
}
