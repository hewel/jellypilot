package io.github.hewel.jellypilot

import android.content.ContextWrapper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.bridge.KeystoreCredentialStore
import io.github.hewel.jellypilot.ffi.CredentialStoreException
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SecureCredentialsTest {
  @Test fun encryptedCredentialsSurviveReopenAndRejectTampering() {
    val application = InstrumentationRegistry.getInstrumentation().targetContext
    val directory = File(application.cacheDir, "credential-boundary-${UUID.randomUUID()}").apply { mkdirs() }
    val context = object : ContextWrapper(application) {
      override fun getNoBackupFilesDir(): File = directory
    }
    val store = KeystoreCredentialStore(context)
    val secret = ByteArray(256) { it.toByte() } // Synthetic bytes, never a real account.
    try {
      store.write(secret)
      assertArrayEquals(secret, KeystoreCredentialStore(context).read())
      val blob = requireNotNull(directory.listFiles()).single { it.isFile }
      val encrypted = blob.readBytes()
      assertFalse((0..encrypted.size - secret.size).any { offset ->
        secret.indices.all { encrypted[offset + it] == secret[it] }
      })
      encrypted[encrypted.lastIndex] = (encrypted.last().toInt() xor 1).toByte()
      blob.writeBytes(encrypted)
      assertThrows(CredentialStoreException.Unavailable::class.java) { KeystoreCredentialStore(context).read() }
      store.delete()
      assertNull(KeystoreCredentialStore(context).read())
    } finally {
      directory.deleteRecursively()
    }
  }
}
