//! The Java VM and the app's Context, for cpal (decision D79).
//!
//! cpal's Android back end calls into Java (AudioManager) through
//! `ndk_context`, which apps built on `android-activity` set up before
//! `main`. A React Native app has no such start, so opening a speaker
//! panicked with "android context was not initialized", ending the audio
//! thread without a word. The voice-service module hands both over once,
//! as the app starts (`CoreContext.attach` in its Kotlin).

use std::ptr;
use std::sync::Once;

use jni_sys::{JNI_OK, JNIEnv, JavaVM, jclass, jobject};

static ATTACHED: Once = Once::new();

/// `dev.opencord.voiceservice.CoreContext.attachNative(context)`. Later calls,
/// after a JavaScript reload, do nothing: `ndk_context` takes one context
/// for the process.
///
/// # Safety
///
/// Called by the Java VM, with its `JNIEnv` for this thread and a live
/// `Context`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_opencord_voiceservice_CoreContext_attachNative(
    env: *mut JNIEnv,
    _class: jclass,
    context: jobject,
) {
    ATTACHED.call_once(|| {
        // SAFETY: `env` is the VM's environment for this thread (above);
        // its function table outlives the call.
        let functions = unsafe { &**env };
        let (Some(get_vm), Some(new_global_ref)) = (functions.GetJavaVM, functions.NewGlobalRef)
        else {
            return;
        };
        let mut vm: *mut JavaVM = ptr::null_mut();
        // SAFETY: the table's functions take this `env`, and `vm` is a
        // place for the result.
        if unsafe { get_vm(env, &mut vm) } != JNI_OK || vm.is_null() {
            return;
        }
        // A global reference, so the Context outlives this call; it is
        // kept for the life of the process.
        // SAFETY: `context` is a live local reference passed by the VM.
        let context = unsafe { new_global_ref(env, context) };
        if context.is_null() {
            return;
        }
        // SAFETY: both pointers are valid for the process's life, and the
        // `Once` makes this the only call.
        unsafe { ndk_context::initialize_android_context(vm.cast(), context.cast()) };
    });
}
