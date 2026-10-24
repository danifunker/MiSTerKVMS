//! What the Android app needs from the core outside UniFFI: JNI functions
//! for `com.danifunker.misterkvms.AndroidCore` (`android/`). The app loads the
//! library with `System.loadLibrary("kvms_core")` so the JVM finds them; JNA
//! (the UniFFI bindings) opens the same library.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::{Once, OnceLock};

use jni_sys::{jclass, jlong, jobject, jstring, JNIEnv, JavaVM};

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(env: *mut JNIEnv, surface: jobject) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
}

#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

/// `crate::log` on Android: logcat, tag "kvms" (`adb logcat -s kvms`).
pub(crate) fn log(msg: &str) {
    write(4, msg);
}

/// One line to logcat at `priority` (4 info, 5 warn, 6 error).
fn write(priority: c_int, msg: &str) {
    let text = CString::new(msg.replace('\0', "")).unwrap_or_default();
    unsafe { __android_log_write(priority, c"kvms".as_ptr(), text.as_ptr()) };
}

/// Warnings and errors from the crates we use (wgpu, cpal) through `log`,
/// which nothing shows on Android otherwise.
struct Logcat;

impl ::log::Log for Logcat {
    fn enabled(&self, metadata: &::log::Metadata) -> bool {
        metadata.level() <= ::log::Level::Warn
    }

    fn log(&self, record: &::log::Record) {
        if self.enabled(record.metadata()) {
            let priority = if record.level() == ::log::Level::Error { 6 } else { 5 };
            write(priority, &format!("{}: {}", record.target(), record.args()));
        }
    }

    fn flush(&self) {}
}

static CONTEXT: Once = Once::new();
static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The app's private files folder (`Context.getFilesDir()`): `config_dir`.
pub(crate) fn files_dir() -> Option<PathBuf> {
    FILES_DIR.get().cloned()
}

/// `AndroidCore.init(context, filesDir)`: hands the JVM and the application
/// context to the crates that call into Java (cpal's AAudio backend asks the
/// AudioManager for devices), and says where the settings go. Call once,
/// before creating the Controller.
///
/// # Safety
/// Called by the JVM with a valid env, an Android `Context` and a string.
#[no_mangle]
pub unsafe extern "system" fn Java_com_danifunker_misterkvms_AndroidCore_init(
    env: *mut JNIEnv,
    _: jclass,
    context: jobject,
    files_dir: jstring,
) {
    CONTEXT.call_once(|| {
        if ::log::set_logger(&Logcat).is_ok() {
            ::log::set_max_level(::log::LevelFilter::Warn);
        }
        let chars = ((**env).v1_1.GetStringUTFChars)(env, files_dir, std::ptr::null_mut());
        if !chars.is_null() {
            let dir = CStr::from_ptr(chars).to_string_lossy().into_owned();
            ((**env).v1_1.ReleaseStringUTFChars)(env, files_dir, chars);
            let _ = FILES_DIR.set(PathBuf::from(dir));
        }
        let mut vm: *mut JavaVM = std::ptr::null_mut();
        if ((**env).v1_1.GetJavaVM)(env, &mut vm) != 0 || vm.is_null() {
            crate::log("android: no JavaVM");
            return;
        }
        // A global reference: the context outlives this call.
        let context = ((**env).v1_1.NewGlobalRef)(env, context);
        ndk_context::initialize_android_context(vm.cast(), context.cast());
    });
}

/// `AndroidCore.windowFromSurface(surface)`: the `ANativeWindow*` behind a
/// `SurfaceView`'s `Surface`, for `NativeSurface::Android`. 0 if there's
/// none. Release it with `releaseWindow` after `detach_surface`.
///
/// # Safety
/// Called by the JVM with a valid env and a `android.view.Surface`.
#[no_mangle]
pub unsafe extern "system" fn Java_com_danifunker_misterkvms_AndroidCore_windowFromSurface(
    env: *mut JNIEnv,
    _: jclass,
    surface: jobject,
) -> jlong {
    ANativeWindow_fromSurface(env, surface) as usize as jlong
}

/// `AndroidCore.releaseWindow(window)`.
///
/// # Safety
/// `window` came from `windowFromSurface` and isn't drawn into any more.
#[no_mangle]
pub unsafe extern "system" fn Java_com_danifunker_misterkvms_AndroidCore_releaseWindow(_: *mut JNIEnv, _: jclass, window: jlong) {
    if window != 0 {
        ANativeWindow_release(window as usize as *mut c_void);
    }
}
