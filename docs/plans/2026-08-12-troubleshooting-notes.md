# Troubleshooting Notes

## Network or environment issues

- First identify the class of failure: network, locked file, encoding mismatch, or code error.
- For network failures, record the failing URL/service, status code or error text, and one fallback path before retrying.
- For locked build files on Windows, use a separate target directory for verification instead of fighting the running app.
- For files with mojibake or mixed encodings, avoid broad context patches around Chinese text; patch small ASCII-only anchors or inspect exact line ranges first.
- Do not repeat the same failed edit strategy more than twice. Switch method and leave a short note about why.

## This case

The repeated patch failures were caused by mojibake around Chinese labels/comments, not by the network. Future layout edits in `src-tauri/src/generate/upload.rs` should use narrow anchors around numeric constants or ASCII identifiers.

When a generated share card already exists, the UI should not auto-regenerate a new one from the same button. Keep the existing card available for copy/save, and make regeneration an explicit action only.
