package io.github.hewel.jellypilot;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.res.AssetFileDescriptor;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.Handler;
import android.os.Looper;
import android.os.ParcelFileDescriptor;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.FileOutputStream;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Real Binder/descriptor fixture. Java is intentional: the independently launched
 * test APK does not inherit the target APK's deduplicated Kotlin runtime.
 */
public final class TestMediaProvider extends ContentProvider {
  public static final String AUTHORITY = "io.github.hewel.jellypilot.test.media";
  public static final Uri sampleUri = Uri.parse("content://" + AUTHORITY + "/sample.mkv");
  public static final Uri subtitleUri = Uri.parse("content://" + AUTHORITY + "/sample.eng.srt");
  public static final Uri blockedUri = Uri.parse("content://" + AUTHORITY + "/blocked");
  private static volatile BlockedOpen blocked = new BlockedOpen();

  private static final class BlockedOpen {
    final CountDownLatch entered = new CountDownLatch(1);
    final CountDownLatch cancelled = new CountDownLatch(1);
    final CountDownLatch release = new CountDownLatch(1);
    final CountDownLatch closed = new CountDownLatch(1);
  }

  @Override public boolean onCreate() { return true; }

  // ContentResolver routes read-only opens through the typed API; its default
  // implementation drops the cancellation signal before opening the asset.
  @Override public AssetFileDescriptor openTypedAssetFile(
      Uri uri, String mimeType, Bundle options, CancellationSignal signal) throws FileNotFoundException {
    String segment = uri.getLastPathSegment();
    // Retain this open's latches even if another test resets the fixture.
    BlockedOpen request = "blocked".equals(segment) ? blocked : null;
    if (request != null) {
      request.entered.countDown();
      if (signal != null) signal.setOnCancelListener(request.cancelled::countDown);
      if (!await(request.release, 30)) throw new FileNotFoundException("fixture release timed out");
    }
    String asset;
    if ("sample.mkv".equals(segment) || request != null) asset = "sample.mkv";
    else if ("sample.eng.srt".equals(segment)) asset = "sample.eng.srt";
    else throw new FileNotFoundException("unknown test media");
    try {
      File directory = new File(getContext().getCacheDir(), "test-media");
      directory.mkdirs();
      File file = new File(directory, asset);
      if (!file.isFile()) {
        try (InputStream input = getContext().getAssets().open(asset);
             OutputStream output = new FileOutputStream(file)) {
          byte[] buffer = new byte[8192];
          int count;
          while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
        }
      }
      ParcelFileDescriptor descriptor = request == null
          ? ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
          : ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY,
              new Handler(Looper.getMainLooper()), error -> request.closed.countDown());
      return new AssetFileDescriptor(descriptor, 0, AssetFileDescriptor.UNKNOWN_LENGTH);
    } catch (IOException error) {
      throw new FileNotFoundException("fixture asset unavailable");
    }
  }

  @Override public Bundle call(String method, String arg, Bundle extras) {
    BlockedOpen request = blocked;
    boolean result;
    switch (method) {
      case "reset": blocked = new BlockedOpen(); result = true; break;
      case "entered": result = await(request.entered, 15); break;
      case "cancelled": result = await(request.cancelled, 15); break;
      case "closed": result = await(request.closed, 15); break;
      case "release": request.release.countDown(); result = true; break;
      default: throw new IllegalArgumentException("unknown fixture operation");
    }
    Bundle response = new Bundle();
    response.putBoolean("result", result);
    return response;
  }

  private static boolean await(CountDownLatch latch, int seconds) {
    try { return latch.await(seconds, TimeUnit.SECONDS); }
    catch (InterruptedException error) { Thread.currentThread().interrupt(); return false; }
  }

  @Override public String getType(Uri uri) {
    return "sample.eng.srt".equals(uri.getLastPathSegment()) ? "application/x-subrip" : "video/x-matroska";
  }
  @Override public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) { return null; }
  @Override public Uri insert(Uri uri, ContentValues values) { return null; }
  @Override public int delete(Uri uri, String selection, String[] args) { return 0; }
  @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { return 0; }
}
