// JellyPilot Android libmpv JNI host.
//
// Original implementation for JellyPilot; architecture informed by
// mpv-android (https://github.com/mpv-android/mpv-android, MIT license) and the
// libmpv client API documentation. No mpv-android source is copied.
//
// Surface ownership: libmpv renders through vo=gpu-next onto an ANativeWindow
// obtained from the Java Surface via the `wid` option. Detach is made
// deterministic without the mpv-android surfaceDestroyed race:
//   1. `vo` is set to "null". The option change is applied asynchronously on
//      the mpv core thread, where it runs uninit_video_out() -> vo_destroy()
//      -> ANativeWindow_release().
//   2. `wid` is reset to 0 (synchronous option write).
//   3. A barrier command is queued with mpv_command_async(). The mpv core
//      dispatch queue is FIFO, so the barrier's MPV_EVENT_COMMAND_REPLY is
//      emitted only after the queued VO teardown above has finished.
//   4. nativeDetachSurface() blocks until that reply arrives, then releases
//      the Surface global ref. When it returns, libmpv provably no longer
//      references the ANativeWindow and SurfaceView may destroy it.
//
// Attach reverses this: store a global ref, set `wid`, set `vo` back to
// "gpu-next"; VO reinit runs on the core thread and calls
// ANativeWindow_fromSurface() while the Surface is still valid.

#include <jni.h>
#include <pthread.h>
#include <locale.h>

#include <atomic>
#include <condition_variable>
#include <cstdint>
#include <mutex>
#include <cstring>
#include <string>

#include <android/log.h>
#include <android/native_window_jni.h>

#include <mpv/client.h>

extern "C" {
#include <libavcodec/jni.h>
}

#define LOG_TAG "JellyPilotPlayer"
#define ALOGV(...) __android_log_print(ANDROID_LOG_VERBOSE, LOG_TAG, __VA_ARGS__)
#define ALOGW(...) __android_log_print(ANDROID_LOG_WARN, LOG_TAG, __VA_ARGS__)
#define ALOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

#define JNI_FN(name) \
  Java_io_github_hewel_jellypilot_player_MpvJni_native##name

namespace {

JavaVM *g_vm = nullptr;

// One instance per MpvPlayerHost. All fields except the atomics are touched
// only from the serialized Kotlin executor thread or the event thread as
// documented per member.
struct PlayerInstance {
  mpv_handle *mpv = nullptr;

  // Global ref to the NativeListener; owned by the instance, deleted on destroy.
  jobject listener = nullptr;

  // Cached listener method IDs (valid for the process lifetime).
  static jmethodID onPropertyFlag;
  static jmethodID onPropertyLong;
  static jmethodID onPropertyDouble;
  static jmethodID onPropertyString;
  static jmethodID onPropertyNone;
  static jmethodID onEvent;
  static jmethodID onStartFile;
  static jmethodID onEndFile;
  static jmethodID onLog;

  // Event thread: drains mpv_wait_event and forwards to the listener.
  pthread_t event_thread{};
  std::atomic<bool> event_exit{false};
  bool event_thread_started = false;

  // Surface currently handed to libmpv (global ref), or nullptr.
  jobject surface = nullptr;

  // Detach barrier: the reply_userdata of the in-flight barrier command and
  // the flag the event thread sets when its COMMAND_REPLY arrives.
  std::mutex barrier_mutex;
  std::condition_variable barrier_cv;
  uint64_t barrier_reply_id = 0;
  bool barrier_done = false;

  // Monotonic reply_userdata source for observes and async commands.
  std::atomic<uint64_t> next_reply_id{1};
};

jmethodID PlayerInstance::onPropertyFlag = nullptr;
jmethodID PlayerInstance::onPropertyLong = nullptr;
jmethodID PlayerInstance::onPropertyDouble = nullptr;
jmethodID PlayerInstance::onPropertyString = nullptr;
jmethodID PlayerInstance::onPropertyNone = nullptr;
jmethodID PlayerInstance::onEvent = nullptr;
jmethodID PlayerInstance::onStartFile = nullptr;
jmethodID PlayerInstance::onEndFile = nullptr;
jmethodID PlayerInstance::onLog = nullptr;

constexpr const char *kListenerClass =
    "io/github/hewel/jellypilot/player/MpvJni$NativeListener";

// Baseline applied before mpv_initialize(). The build pipeline pins the mpv
// version, so an option that fails here is a real integration defect: every
// failure aborts creation instead of silently degrading playback.
struct BaselineOption {
  const char *name;
  const char *value;
};

// vo stays "null" until a surface is attached; gpu-next is the only video
// output so HDR/DV capability (libplacebo) is not silently lost. gpu-api and
// gpu-context are ordered fallback lists: Vulkan (androidvk) is preferred for
// HDR-capable output, with the OpenGL Android context as the SDR fallback.
// target-colorspace-hint lets gpu-next derive the target colorspace from the
// negotiated swapchain; no target-trc/prim is forced.
//
// The pinned build compiles with -Dlua=disabled, so Lua-only options (osc,
// ytdl, load-scripts) do not exist and must not appear here — an unknown
// option aborts creation. Only compiled audio outputs are listed
// (audiotrack, aaudio; opensles is disabled in the pinned build).
constexpr BaselineOption kBaselineOptions[] = {
    {"config", "no"},
    {"idle", "yes"},
    // The SDK owns playlists. FFmpeg still handles HLS/DASH within one media
    // timeline, without mpv expanding M3U/PLS into untracked playlist entries.
    {"demuxer", "lavf"},
    {"vo", "null"},
    {"gpu-api", "vulkan,opengl"},
    {"gpu-context", "androidvk,android"},
    {"opengl-es", "yes"},
    {"target-colorspace-hint", "yes"},
    {"hwdec", "mediacodec,mediacodec-copy"},
    {"hwdec-codecs", "h264,hevc,mpeg4,mpeg2video,vp8,vp9,av1"},
    {"ao", "audiotrack,aaudio"},
    {"audio-set-media-role", "yes"},
    {"tls-verify", "yes"},
    {"keep-open", "yes"},
    {"force-window", "no"},
    {"input-default-bindings", "no"},
    {"input-vo-keyboard", "no"},
    {"save-position-on-quit", "no"},
};

JNIEnv *attach_env() {
  JNIEnv *env = nullptr;
  if (g_vm->GetEnv(reinterpret_cast<void **>(&env), JNI_VERSION_1_6) ==
      JNI_OK) {
    return env;
  }
  if (g_vm->AttachCurrentThread(&env, nullptr) != JNI_OK) {
    return nullptr;
  }
  return env;
}

void detach_env() { g_vm->DetachCurrentThread(); }

void throw_player_exception(JNIEnv *env, const char *message) {
  jclass cls = env->FindClass(
      "io/github/hewel/jellypilot/player/PlayerHostException");
  if (cls) {
    env->ThrowNew(cls, message);
    env->DeleteLocalRef(cls);
  }
}

PlayerInstance *from_handle(jlong handle) {
  return reinterpret_cast<PlayerInstance *>(static_cast<intptr_t>(handle));
}

void call_listener_void(PlayerInstance *instance, JNIEnv *env, jmethodID method,
                        jstring name) {
  env->CallVoidMethod(instance->listener, method, name);
  if (env->ExceptionCheck()) {
    env->ExceptionDescribe();
    env->ExceptionClear();
  }
}

void forward_property(PlayerInstance *instance, JNIEnv *env,
                      mpv_event_property *prop) {
  jstring jname = env->NewStringUTF(prop->name ? prop->name : "");
  switch (prop->format) {
    case MPV_FORMAT_FLAG: {
      int value = prop->data ? *static_cast<int *>(prop->data) : 0;
      env->CallVoidMethod(instance->listener, PlayerInstance::onPropertyFlag,
                          jname, value);
      break;
    }
    case MPV_FORMAT_INT64: {
      int64_t value = prop->data ? *static_cast<int64_t *>(prop->data) : 0;
      env->CallVoidMethod(instance->listener, PlayerInstance::onPropertyLong,
                          jname, static_cast<jlong>(value));
      break;
    }
    case MPV_FORMAT_DOUBLE: {
      double value = prop->data ? *static_cast<double *>(prop->data) : 0.0;
      env->CallVoidMethod(instance->listener, PlayerInstance::onPropertyDouble,
                          jname, static_cast<jdouble>(value));
      break;
    }
    case MPV_FORMAT_STRING:
    case MPV_FORMAT_OSD_STRING: {
      const char *text =
          prop->data ? *static_cast<char **>(prop->data) : nullptr;
      jstring jvalue = env->NewStringUTF(text ? text : "");
      env->CallVoidMethod(instance->listener, PlayerInstance::onPropertyString,
                          jname, jvalue);
      env->DeleteLocalRef(jvalue);
      break;
    }
    default:
      env->CallVoidMethod(instance->listener, PlayerInstance::onPropertyNone,
                          jname);
      break;
  }
  if (env->ExceptionCheck()) {
    env->ExceptionDescribe();
    env->ExceptionClear();
  }
  env->DeleteLocalRef(jname);
}

void *event_thread_main(void *arg) {
  auto *instance = static_cast<PlayerInstance *>(arg);
  JNIEnv *env = attach_env();
  if (!env) {
    ALOGE("event thread failed to attach to JVM");
    return nullptr;
  }

  while (!instance->event_exit.load(std::memory_order_acquire)) {
    mpv_event *event = mpv_wait_event(instance->mpv, -1.0);
    if (instance->event_exit.load(std::memory_order_acquire)) {
      break;
    }
    if (event->event_id == MPV_EVENT_NONE) {
      continue;
    }

    switch (event->event_id) {
      case MPV_EVENT_PROPERTY_CHANGE:
        forward_property(instance, env,
                         static_cast<mpv_event_property *>(event->data));
        break;
      case MPV_EVENT_COMMAND_REPLY: {
        // Barrier replies are consumed here; other replies are ignored.
        std::lock_guard<std::mutex> lock(instance->barrier_mutex);
        if (instance->barrier_reply_id != 0 &&
            event->reply_userdata == instance->barrier_reply_id) {
          instance->barrier_done = true;
          instance->barrier_cv.notify_all();
        }
        break;
      }
      case MPV_EVENT_START_FILE: {
        auto *start = static_cast<mpv_event_start_file *>(event->data);
        env->CallVoidMethod(instance->listener, PlayerInstance::onStartFile,
                            static_cast<jlong>(start->playlist_entry_id));
        if (env->ExceptionCheck()) {
          env->ExceptionDescribe();
          env->ExceptionClear();
        }
        break;
      }
      case MPV_EVENT_END_FILE: {
        auto *end = static_cast<mpv_event_end_file *>(event->data);
        env->CallVoidMethod(instance->listener, PlayerInstance::onEndFile,
                            static_cast<jlong>(end->playlist_entry_id),
                            static_cast<jint>(end->reason),
                            static_cast<jint>(end->error));
        if (env->ExceptionCheck()) {
          env->ExceptionDescribe();
          env->ExceptionClear();
        }
        break;
      }
      case MPV_EVENT_LOG_MESSAGE: {
        auto *msg = static_cast<mpv_event_log_message *>(event->data);
        // Strip invalid UTF-8 lead bytes Java would reject (copy; text is const).
        std::string text = msg->text ? msg->text : "";
        for (char &c : text) {
          auto byte = static_cast<unsigned char>(c);
          if (byte == 0xc0 || byte == 0xc1 || byte >= 0xf5) {
            c = '?';
          }
        }
        jstring jprefix = env->NewStringUTF(msg->prefix ? msg->prefix : "");
        jstring jtext = env->NewStringUTF(text.c_str());
        env->CallVoidMethod(instance->listener, PlayerInstance::onLog, jprefix,
                            static_cast<jint>(msg->log_level), jtext);
        if (env->ExceptionCheck()) {
          env->ExceptionDescribe();
          env->ExceptionClear();
        }
        env->DeleteLocalRef(jprefix);
        env->DeleteLocalRef(jtext);
        break;
      }
      default:
        env->CallVoidMethod(instance->listener, PlayerInstance::onEvent,
                            static_cast<jint>(event->event_id));
        if (env->ExceptionCheck()) {
          env->ExceptionDescribe();
          env->ExceptionClear();
        }
        break;
    }
  }

  detach_env();
  return nullptr;
}

// Queue a no-op barrier command and wait for its reply. Returns true when the
// core thread confirmed it processed everything queued before the barrier.
bool run_barrier(PlayerInstance *instance, int timeout_ms) {
  uint64_t reply_id =
      instance->next_reply_id.fetch_add(1, std::memory_order_relaxed);
  {
    std::lock_guard<std::mutex> lock(instance->barrier_mutex);
    instance->barrier_reply_id = reply_id;
    instance->barrier_done = false;
  }
  const char *args[] = {"set", "user-data/jellypilot/barrier", "1", nullptr};
  if (mpv_command_async(instance->mpv, reply_id, args) < 0) {
    std::lock_guard<std::mutex> lock(instance->barrier_mutex);
    instance->barrier_reply_id = 0;
    return false;
  }
  std::unique_lock<std::mutex> lock(instance->barrier_mutex);
  bool done = instance->barrier_cv.wait_for(
      lock, std::chrono::milliseconds(timeout_ms),
      [&] { return instance->barrier_done; });
  instance->barrier_reply_id = 0;
  return done;
}

int set_option_string(mpv_handle *mpv, const char *name, const char *value) {
  int result = mpv_set_option_string(mpv, name, value);
  if (result < 0) {
    ALOGE("set_option %s=%s failed: %s", name, value, mpv_error_string(result));
  }
  return result;
}

}  // namespace

extern "C" {

JNIEXPORT jlong JNICALL JNI_FN(Create)(JNIEnv *env, jclass /*clazz*/,
                                     jobject app_context, jobject listener,
                                     jstring cache_dir, jstring tls_ca_file) {
  setlocale(LC_NUMERIC, "C");

  if (!g_vm) {
    env->GetJavaVM(&g_vm);
  }
  av_jni_set_java_vm(g_vm, nullptr);
  jobject global_ctx = env->NewGlobalRef(app_context);
  if (global_ctx) {
    av_jni_set_android_app_ctx(global_ctx, nullptr);
  }

  auto *instance = new PlayerInstance();
  instance->listener = env->NewGlobalRef(listener);
  if (!instance->listener) {
    delete instance;
    throw_player_exception(env, "failed to reference listener");
    return 0;
  }

  jclass listener_cls = env->FindClass(kListenerClass);
  if (!listener_cls) {
    env->DeleteGlobalRef(instance->listener);
    delete instance;
    return 0;  // FindClass already threw NoClassDefFoundError.
  }
  PlayerInstance::onPropertyFlag = env->GetMethodID(
      listener_cls, "onPropertyFlag", "(Ljava/lang/String;I)V");
  PlayerInstance::onPropertyLong = env->GetMethodID(
      listener_cls, "onPropertyLong", "(Ljava/lang/String;J)V");
  PlayerInstance::onPropertyDouble = env->GetMethodID(
      listener_cls, "onPropertyDouble", "(Ljava/lang/String;D)V");
  PlayerInstance::onPropertyString = env->GetMethodID(
      listener_cls, "onPropertyString", "(Ljava/lang/String;Ljava/lang/String;)V");
  PlayerInstance::onPropertyNone = env->GetMethodID(
      listener_cls, "onPropertyNone", "(Ljava/lang/String;)V");
  PlayerInstance::onEvent =
      env->GetMethodID(listener_cls, "onEvent", "(I)V");
  PlayerInstance::onStartFile =
      env->GetMethodID(listener_cls, "onStartFile", "(J)V");
  PlayerInstance::onEndFile =
      env->GetMethodID(listener_cls, "onEndFile", "(JII)V");
  PlayerInstance::onLog = env->GetMethodID(
      listener_cls, "onLog", "(Ljava/lang/String;ILjava/lang/String;)V");
  env->DeleteLocalRef(listener_cls);
  if (!PlayerInstance::onPropertyFlag || !PlayerInstance::onPropertyLong ||
      !PlayerInstance::onPropertyDouble || !PlayerInstance::onPropertyString ||
      !PlayerInstance::onPropertyNone || !PlayerInstance::onEvent ||
      !PlayerInstance::onStartFile || !PlayerInstance::onEndFile ||
      !PlayerInstance::onLog) {
    env->DeleteGlobalRef(instance->listener);
    delete instance;
    throw_player_exception(env, "listener method lookup failed");
    return 0;
  }

  instance->mpv = mpv_create();
  if (!instance->mpv) {
    env->DeleteGlobalRef(instance->listener);
    delete instance;
    throw_player_exception(env, "mpv_create failed");
    return 0;
  }

  for (const BaselineOption &option : kBaselineOptions) {
    if (set_option_string(instance->mpv, option.name, option.value) < 0) {

      char message[256];
      snprintf(message, sizeof(message), "mpv option %s rejected",
               option.name);
      mpv_terminate_destroy(instance->mpv);
      env->DeleteGlobalRef(instance->listener);
      delete instance;
      throw_player_exception(env, message);
      return 0;
    }
  }

  if (cache_dir) {
    const char *dir = env->GetStringUTFChars(cache_dir, nullptr);
    set_option_string(instance->mpv, "gpu-shader-cache-dir", dir);
    set_option_string(instance->mpv, "icc-cache-dir", dir);
    env->ReleaseStringUTFChars(cache_dir, dir);
  }
  if (tls_ca_file) {
    const char *ca = env->GetStringUTFChars(tls_ca_file, nullptr);
    set_option_string(instance->mpv, "tls-ca-file", ca);
    env->ReleaseStringUTFChars(tls_ca_file, ca);
  }

  mpv_request_log_messages(instance->mpv, "info");

  if (mpv_initialize(instance->mpv) < 0) {
    mpv_terminate_destroy(instance->mpv);
    env->DeleteGlobalRef(instance->listener);
    delete instance;
    throw_player_exception(env, "mpv_initialize failed");
    return 0;
  }

  if (pthread_create(&instance->event_thread, nullptr, event_thread_main,
                     instance) != 0) {
    mpv_terminate_destroy(instance->mpv);
    env->DeleteGlobalRef(instance->listener);
    delete instance;
    throw_player_exception(env, "event thread creation failed");
    return 0;
  }
  instance->event_thread_started = true;
  pthread_setname_np(instance->event_thread, "jp_mpv_events");

  return reinterpret_cast<jlong>(instance);
}

JNIEXPORT void JNICALL JNI_FN(Destroy)(JNIEnv *env, jclass /*clazz*/,
                                     jlong handle) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance) {
    return;
  }

  instance->event_exit.store(true, std::memory_order_release);
  mpv_wakeup(instance->mpv);
  if (instance->event_thread_started) {
    pthread_join(instance->event_thread, nullptr);
  }

  mpv_terminate_destroy(instance->mpv);
  instance->mpv = nullptr;
  // The core, including VO teardown, has now stopped using this jobject.
  if (instance->surface) {
    env->DeleteGlobalRef(instance->surface);
    instance->surface = nullptr;
  }

  env->DeleteGlobalRef(instance->listener);
  instance->listener = nullptr;
  delete instance;
}

JNIEXPORT void JNICALL JNI_FN(AttachSurface)(JNIEnv *env, jclass /*clazz*/,
                                           jlong handle, jobject surface) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv || !surface) {
    return;
  }

  jobject global_surface = env->NewGlobalRef(surface);
  if (!global_surface) {
    throw_player_exception(env, "failed to reference surface");
    return;
  }

  if (instance->surface) {
    // A previous surface is still referenced by `wid`; a queued VO reinit may
    // not have consumed it yet, so it must be relinquished before the global
    // ref can be dropped safely.
    int64_t no_wid = 0;
    if (mpv_set_property_string(instance->mpv, "vo", "null") < 0 ||
        mpv_set_option(instance->mpv, "wid", MPV_FORMAT_INT64, &no_wid) < 0 ||
        !run_barrier(instance, 5000)) {
      env->DeleteGlobalRef(global_surface);
      throw_player_exception(env, "prior surface relinquishment failed");
      return;
    }
    env->DeleteGlobalRef(instance->surface);
  }
  instance->surface = global_surface;

  int64_t wid = reinterpret_cast<intptr_t>(global_surface);
  if (mpv_set_option(instance->mpv, "wid", MPV_FORMAT_INT64, &wid) < 0) {
    throw_player_exception(env, "mpv rejected the new surface");
    return;
  }
  // Queued VO reinit runs on the core thread while the Surface is valid.
  if (mpv_set_property_string(instance->mpv, "vo", "gpu-next") < 0) {
    throw_player_exception(env, "mpv rejected video output initialization");
  }
}

JNIEXPORT void JNICALL JNI_FN(DetachSurface)(JNIEnv *env, jclass /*clazz*/,
                                           jlong handle) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv || !instance->surface) {
    return;
  }

  // Queue VO teardown on the core thread, then wait on a barrier reply that
  // can only be emitted after vo_destroy() released the ANativeWindow.
  int64_t wid = 0;
  if (mpv_set_property_string(instance->mpv, "vo", "null") < 0 ||
      mpv_set_option(instance->mpv, "wid", MPV_FORMAT_INT64, &wid) < 0) {
    throw_player_exception(env, "mpv rejected surface detachment");
    return;
  }
  if (!run_barrier(instance, 5000)) {
    // The core is unresponsive; keep the global ref so a late VO teardown
    // cannot dereference a freed jobject. The Surface itself is already
    // invalid from SurfaceFlinger's perspective either way.
    throw_player_exception(env, "surface detach barrier timed out");
    return;
  }
  env->DeleteGlobalRef(instance->surface);
  instance->surface = nullptr;
}


// Marshals a Java String[] into a null-terminated argv. Returns false (with
// partial releases undone) when the array is empty, oversized, or an element
// is null.
static bool marshal_string_args(JNIEnv *env, jobjectArray jargs,
                                const char **args, jstring *strings,
                                jsize *out_len) {
  jsize len = env->GetArrayLength(jargs);
  if (len <= 0 || len > 32) {
    return false;
  }
  for (jsize i = 0; i < len; ++i) {
    strings[i] = static_cast<jstring>(env->GetObjectArrayElement(jargs, i));
    if (!strings[i]) {
      for (jsize j = 0; j < i; ++j) {
        env->ReleaseStringUTFChars(strings[j], args[j]);
        env->DeleteLocalRef(strings[j]);
      }
      return false;
    }
    args[i] = env->GetStringUTFChars(strings[i], nullptr);
  }
  *out_len = len;
  return true;
}

static void release_string_args(JNIEnv *env, const char **args,
                                jstring *strings, jsize len) {
  for (jsize i = 0; i < len; ++i) {
    env->ReleaseStringUTFChars(strings[i], args[i]);
    env->DeleteLocalRef(strings[i]);
  }
}

JNIEXPORT jint JNICALL JNI_FN(Command)(JNIEnv *env, jclass /*clazz*/,
                                       jlong handle, jobjectArray jargs) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return MPV_ERROR_UNINITIALIZED;
  }

  const char *args[33] = {nullptr};
  jstring strings[32] = {nullptr};
  jsize len = 0;
  if (!marshal_string_args(env, jargs, args, strings, &len)) {
    return MPV_ERROR_INVALID_PARAMETER;
  }

  int result = mpv_command(instance->mpv, args);
  release_string_args(env, args, strings, len);
  return result;
}

// Runs `loadfile` via mpv_command_ret so the accepted playlist_entry_id comes
// back with the reply. Returns the id (> 0) on success, or the mpv_error code
// (< 0) when the command was refused before an entry was installed.
JNIEXPORT jlong JNICALL JNI_FN(LoadFile)(JNIEnv *env, jclass /*clazz*/,
                                         jlong handle, jobjectArray jargs) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return MPV_ERROR_UNINITIALIZED;
  }

  const char *args[33] = {nullptr};
  jstring strings[32] = {nullptr};
  jsize len = 0;
  if (!marshal_string_args(env, jargs, args, strings, &len)) {
    return MPV_ERROR_INVALID_PARAMETER;
  }

  mpv_node result_node{};
  int result = mpv_command_ret(instance->mpv, args, &result_node);
  release_string_args(env, args, strings, len);
  if (result < 0) {
    return result;
  }

  int64_t entry_id = 0;
  if (result_node.format == MPV_FORMAT_NODE_MAP && result_node.u.list) {
    mpv_node_list *list = result_node.u.list;
    for (int i = 0; i < list->num; ++i) {
      if (list->keys[i] &&
          strcmp(list->keys[i], "playlist_entry_id") == 0 &&
          list->values[i].format == MPV_FORMAT_INT64) {
        entry_id = list->values[i].u.int64;
        break;
      }
    }
  }
  mpv_free_node_contents(&result_node);
  // A successful loadfile always reports an entry id; 0 means the reply was
  // malformed, which is a real integration defect, not a silent success.
  return entry_id > 0 ? entry_id : MPV_ERROR_GENERIC;
}

JNIEXPORT jint JNICALL JNI_FN(SetPropertyString)(JNIEnv *env, jclass /*clazz*/,
                                                 jlong handle, jstring jname,
                                                 jstring jvalue) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return MPV_ERROR_UNINITIALIZED;
  }
  const char *name = env->GetStringUTFChars(jname, nullptr);
  const char *value = env->GetStringUTFChars(jvalue, nullptr);
  int result = mpv_set_property_string(instance->mpv, name, value);
  env->ReleaseStringUTFChars(jname, name);
  env->ReleaseStringUTFChars(jvalue, value);
  return result;
}

JNIEXPORT jint JNICALL JNI_FN(SetPropertyFlag)(JNIEnv *env, jclass /*clazz*/,
                                               jlong handle, jstring jname,
                                               jboolean value) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return MPV_ERROR_UNINITIALIZED;
  }
  const char *name = env->GetStringUTFChars(jname, nullptr);
  int flag = value == JNI_TRUE ? 1 : 0;
  int result = mpv_set_property(instance->mpv, name, MPV_FORMAT_FLAG, &flag);
  env->ReleaseStringUTFChars(jname, name);
  return result;
}

JNIEXPORT jint JNICALL JNI_FN(SetPropertyDouble)(JNIEnv *env, jclass /*clazz*/,
                                                 jlong handle, jstring jname,
                                                 jdouble value) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return MPV_ERROR_UNINITIALIZED;
  }
  const char *name = env->GetStringUTFChars(jname, nullptr);
  double v = value;
  int result = mpv_set_property(instance->mpv, name, MPV_FORMAT_DOUBLE, &v);
  env->ReleaseStringUTFChars(jname, name);
  return result;
}

JNIEXPORT jstring JNICALL JNI_FN(GetPropertyString)(JNIEnv *env,
                                                  jclass /*clazz*/,
                                                  jlong handle,
                                                  jstring jname) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return nullptr;
  }
  const char *name = env->GetStringUTFChars(jname, nullptr);
  char *value = mpv_get_property_string(instance->mpv, name);
  env->ReleaseStringUTFChars(jname, name);
  if (!value) {
    return nullptr;
  }
  jstring result = env->NewStringUTF(value);
  mpv_free(value);
  return result;
}

JNIEXPORT jint JNICALL JNI_FN(SetHttpHeaders)(JNIEnv *env, jclass /*clazz*/,
                                              jlong handle,
                                              jobjectArray jheaders) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return MPV_ERROR_UNINITIALIZED;
  }

  jsize count = jheaders ? env->GetArrayLength(jheaders) : 0;
  mpv_node_list list{};
  list.num = count;
  list.values = count > 0 ? new mpv_node[count] : nullptr;
  jstring *strings = count > 0 ? new jstring[count] : nullptr;

  for (jsize i = 0; i < count; ++i) {
    strings[i] = static_cast<jstring>(env->GetObjectArrayElement(jheaders, i));
    const char *text =
        strings[i] ? env->GetStringUTFChars(strings[i], nullptr) : nullptr;
    list.values[i].format = MPV_FORMAT_STRING;
    list.values[i].u.string = const_cast<char *>(text ? text : "");
  }

  mpv_node node{};
  node.format = MPV_FORMAT_NODE_ARRAY;
  node.u.list = &list;
  int result = mpv_set_property(instance->mpv, "http-header-fields",
                                MPV_FORMAT_NODE, &node);

  for (jsize i = 0; i < count; ++i) {
    if (strings[i]) {
      env->ReleaseStringUTFChars(strings[i], list.values[i].u.string);
      env->DeleteLocalRef(strings[i]);
    }
  }
  delete[] strings;
  delete[] list.values;
  return result;
}

JNIEXPORT void JNICALL JNI_FN(ObserveProperty)(JNIEnv *env, jclass /*clazz*/,
                                               jlong handle, jstring jname,
                                               jint format) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return;
  }
  const char *name = env->GetStringUTFChars(jname, nullptr);
  uint64_t reply =
      instance->next_reply_id.fetch_add(1, std::memory_order_relaxed);
  mpv_observe_property(instance->mpv, reply, name,
                       static_cast<mpv_format>(format));
  env->ReleaseStringUTFChars(jname, name);
}

JNIEXPORT jboolean JNICALL JNI_FN(Barrier)(JNIEnv * /*env*/, jclass /*clazz*/,
                                           jlong handle, jint timeout_ms) {
  PlayerInstance *instance = from_handle(handle);
  if (!instance || !instance->mpv) {
    return JNI_FALSE;
  }
  return run_barrier(instance, timeout_ms) ? JNI_TRUE : JNI_FALSE;
}


}  // extern "C"
