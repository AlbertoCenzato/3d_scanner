window.save_streaming_file = async function(wasm_memory, dataPointer, dataLength, suggestedName) {
    // Ask user where to save the file
    const handle = await window.showSaveFilePicker({
        suggestedName,
        types: [{ description: 'Binary file', accept: { 'application/octet-stream': ['.bin'] } }],
    });

    const writable = await handle.createWritable();

    // Create a JS view directly into WASM memory
    const mem = new Uint8Array(wasm_memory.buffer, dataPointer, dataLength);

    // Write directly to disk — ZERO COPY
    await writable.write(mem);

    // Close the file
    await writable.close();
};

// Synchronous fallback that triggers a download using a Blob and an anchor click.
// This does not require async/await and can be called from non-async Rust code.
window.save_streaming_file_sync = function(wasm_memory, dataPointer, dataLength, suggestedName) {
    // Create a JS view directly into WASM memory
    const mem = new Uint8Array(wasm_memory.buffer, dataPointer, dataLength);

    // Create a blob from the memory view (this will copy the data once)
    const blob = new Blob([mem], { type: 'application/octet-stream' });

    // Create an object URL and click an anchor to trigger download
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.style.display = 'none';
    a.href = url;
    a.download = suggestedName || 'download.bin';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    return true;
};

// Also expose on globalThis for environments where `window` isn't the global root
try {
    if (typeof globalThis !== 'undefined') {
        globalThis.save_streaming_file = window.save_streaming_file;
        globalThis.save_streaming_file_sync = window.save_streaming_file_sync;
    }
} catch (e) {
    // ignore
}
