use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// JS bindings for file saving helper implemented in `assets/save_streaming_file.js`.
#[wasm_bindgen]
extern "C" {
    /// Call the JS save function which returns a Promise.
    #[wasm_bindgen(js_namespace = window, js_name = save_streaming_file)]
    fn save_streaming_file_js(
        wasm_memory: JsValue,
        data_pointer: u32,
        data_length: u32,
        suggested_name: &str,
    ) -> js_sys::Promise;
    /// Synchronous helper that triggers a download via Blob+anchor (no Promise).
    #[wasm_bindgen(js_namespace = window, js_name = save_streaming_file_sync)]
    fn save_streaming_file_sync_js(
        wasm_memory: JsValue,
        data_pointer: u32,
        data_length: u32,
        suggested_name: &str,
    ) -> JsValue;
}

/// Save a file by passing pointer, length and suggested name. Awaits the JS Promise.
pub async fn save_streaming_file(ptr: u32, len: u32, suggested_name: &str) -> Result<(), JsValue> {
    let wasm_memory = wasm_bindgen::memory();
    let promise = save_streaming_file_js(wasm_memory, ptr, len, suggested_name);
    JsFuture::from(promise).await?;
    Ok(())
}

/// Blocking (non-async) variant that calls the synchronous JS helper.
///
/// This can be called from non-async Rust code compiled to wasm. It relies on
/// the JS helper creating a Blob and programmatically clicking an anchor element
/// to trigger the browser's download UI. This will copy the data once into the
/// Blob on the JS side.
pub fn save_streaming_file_blocking(
    ptr: u32,
    len: u32,
    suggested_name: &str,
) -> Result<(), JsValue> {
    let wasm_memory = wasm_bindgen::memory();
    // Call the sync JS helper. It returns a truthy value on success.
    let _ret = save_streaming_file_sync_js(wasm_memory, ptr, len, suggested_name);
    Ok(())
}

/// Convenience blocking helper that accepts an owned `Vec<u8>` and saves it.
///
/// This function boxes the Vec to obtain a stable heap allocation, calls the
/// synchronous JS helper which reads from wasm memory immediately, then
/// reconstructs and drops the box to free memory before returning.
pub fn save_vec_u8_blocking(data: Vec<u8>, suggested_name: &str) -> Result<(), JsValue> {
    let boxed: Box<[u8]> = data.into_boxed_slice();
    let len = boxed.len() as u32;
    let ptr = boxed.as_ptr() as usize as u32;

    // We don't need to leak here because the call is synchronous and will
    // complete before we continue. Call the sync helper directly.
    let wasm_memory = wasm_bindgen::memory();
    let _ = save_streaming_file_sync_js(wasm_memory, ptr, len, suggested_name);

    // Reconstruct the box and drop it to free memory.
    unsafe {
        let _ = Box::from_raw(Box::into_raw(boxed));
    }

    Ok(())
}
