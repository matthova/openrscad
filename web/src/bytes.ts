// Base64 <-> bytes helpers for binary assets (imported STL/3MF). Binary files
// can't ride the string-typed engine file channel or localStorage as raw bytes,
// so they're carried as standard base64 (RFC 4648) — the same encoding the Rust
// engine decodes on the other side.
//
// `btoa`/`atob` operate on binary strings and choke on large inputs (and on any
// code point > 0xFF), so we chunk manually over the byte array instead.

const CHUNK = 0x8000; // 32 KiB — keeps String.fromCharCode argument lists sane

/** Encode bytes as a base64 string (no line breaks). */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

/** Decode a base64 string back to bytes. Throws if the input isn't valid base64. */
export function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}
