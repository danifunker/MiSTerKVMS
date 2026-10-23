//! What the Android app needs from the core outside UniFFI: JNI functions
//! for `com.danifunker.misterkvms.AndroidCore` (`android/`). The app loads the
//! library with `System.loadLibrary("kvms_core")` so the JVM finds them; JNA
//! (the UniFFI bindings) opens the same library.

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::Once;

use jni_sys::{jclass, jlong, jobject, JNIEnv, JavaVM};

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
    const INFO: c_int = 4;
    let text = CString::new(msg.replace('\0', "")).unwrap_or_default();
    unsafe { __android_log_write(INFO, c"kvms".as_ptr(), text.as_ptr()) };
}

static CONTEXT: Once = Once::new();

/// `AndroidCore.init(context)`: hands the JVM and the application context to the
/// crates that call into Java (cpal's AAudio backend asks the AudioManager
/// for devices). Call once, before anything plays sound.
///
/// # Safety
/// Called by the JVM with a valid env and an Android `Context`.
#[no_mangle]
pub unsafe extern "system" fn Java_com_danifunker_misterkvms_AndroidCore_init(env: *mut JNIEnv, _: jclass, context: jobject) {
    CONTEXT.call_once(|| {
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
