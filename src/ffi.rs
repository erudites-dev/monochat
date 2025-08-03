#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::missing_safety_doc)]
#![allow(unsafe_attr_outside_unsafe)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use futures::StreamExt;
use jni::{
    JNIEnv, JavaVM,
    objects::{JClass, JString},
    sys::{JNI_VERSION_1_8, jint, jlong, jstring},
};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::{Message, source};

// Global runtime for async operations
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static STREAM_COUNTER: AtomicU64 = AtomicU64::new(1);
static STREAMS: OnceLock<std::sync::Mutex<HashMap<u64, StreamHandle>>> = OnceLock::new();

struct StreamHandle {
    sender: mpsc::UnboundedSender<()>,
    receiver: std::sync::Mutex<mpsc::UnboundedReceiver<Message>>,
}

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("Failed to create Tokio runtime"))
}

fn get_streams() -> &'static std::sync::Mutex<HashMap<u64, StreamHandle>> {
    STREAMS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

// Helper function to convert Rust String to Java String
fn to_java_string(env: &mut JNIEnv, s: &str) -> Result<jstring, jni::errors::Error> {
    Ok(env.new_string(s)?.into_raw())
}

// Helper function to convert Java String to Rust String
fn from_java_string(env: &mut JNIEnv, s: JString) -> Result<String, jni::errors::Error> {
    Ok(env.get_string(&s)?.into())
}

// Message class methods
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_Message_getSender(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jstring {
    let message = unsafe { &*(ptr as *const Message) };
    match to_java_string(&mut env, &message.sender) {
        Ok(s) => s,
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_Message_getContent(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jstring {
    let message = unsafe { &*(ptr as *const Message) };
    match &message.content {
        Some(content) => match to_java_string(&mut env, content) {
            Ok(s) => s,
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_Message_getDonated(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jlong {
    let message = unsafe { &*(ptr as *const Message) };
    message.donated.unwrap_or(0) as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_Message_hasDonation(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jint {
    let message = unsafe { &*(ptr as *const Message) };
    if message.donated.is_some() { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_Message_free(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if ptr != 0 {
        unsafe {
            let _ = Box::from_raw(ptr as *mut Message);
        }
    }
}

// MonoChat class methods
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_MonoChat_init(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    // Initialize the runtime
    let _runtime = get_runtime();

    // Initialize streams HashMap
    let _streams = get_streams();

    0 // Success
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_MonoChat_connectChzzkNative(
    mut env: JNIEnv,
    _class: JClass,
    url: JString,
) -> jlong {
    let url_str = match from_java_string(&mut env, url) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let runtime = get_runtime();
    let stream_id = STREAM_COUNTER.fetch_add(1, Ordering::SeqCst);

    let (message_tx, message_rx) = mpsc::unbounded_channel();
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel();

    runtime.spawn(async move {
        let stream_result = source::chzzk::new(url_str).await;
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(_) => return,
        };

        loop {
            tokio::select! {
                message = stream.next() => {
                    match message {
                        Some(msg) => {
                            if message_tx.send(msg).is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = stop_rx.recv() => {
                    break;
                }
            }
        }
    });

    {
        let streams = get_streams();
        let mut streams_guard = streams.lock().unwrap();
        streams_guard.insert(
            stream_id,
            StreamHandle {
                sender: stop_tx,
                receiver: std::sync::Mutex::new(message_rx),
            },
        );
    }

    stream_id as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_MonoChat_connectSoopNative(
    mut env: JNIEnv,
    _class: JClass,
    url: JString,
) -> jlong {
    let url_str = match from_java_string(&mut env, url) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let runtime = get_runtime();
    let stream_id = STREAM_COUNTER.fetch_add(1, Ordering::SeqCst);

    let (message_tx, message_rx) = mpsc::unbounded_channel();
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel();

    runtime.spawn(async move {
        let stream_result = source::soop::new(url_str).await;
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(_) => return,
        };

        loop {
            tokio::select! {
                message = stream.next() => {
                    match message {
                        Some(msg) => {
                            if message_tx.send(msg).is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = stop_rx.recv() => {
                    break;
                }
            }
        }
    });

    {
        let streams = get_streams();
        let mut streams_guard = streams.lock().unwrap();
        streams_guard.insert(
            stream_id,
            StreamHandle {
                sender: stop_tx,
                receiver: std::sync::Mutex::new(message_rx),
            },
        );
    }

    stream_id as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_MonoChat_nextMessage(
    _env: JNIEnv,
    _class: JClass,
    stream_id: jlong,
) -> jlong {
    let streams = get_streams();
    let streams_guard = streams.lock().unwrap();

    if let Some(handle) = streams_guard.get(&(stream_id as u64)) {
        let mut receiver_guard = handle.receiver.lock().unwrap();
        match receiver_guard.try_recv() {
            Ok(message) => {
                let boxed_message = Box::new(message);
                Box::into_raw(boxed_message) as jlong
            }
            Err(_) => 0, // No message available
        }
    } else {
        -1 // Invalid stream ID
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aperso_monochat_MonoChat_closeStreamNative(
    _env: JNIEnv,
    _class: JClass,
    stream_id: jlong,
) -> jint {
    let streams = get_streams();
    let mut streams_guard = streams.lock().unwrap();

    if let Some(handle) = streams_guard.remove(&(stream_id as u64)) {
        let _ = handle.sender.send(());
        // The receiver will be dropped automatically
        0 // Success
    } else {
        -1 // Invalid stream ID
    }
}

// JNI OnLoad function
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(_vm: JavaVM, _reserved: *mut std::ffi::c_void) -> jint {
    JNI_VERSION_1_8
}
