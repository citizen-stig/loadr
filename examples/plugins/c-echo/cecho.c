/*
 * c-echo — an example loadr protocol plugin written in plain C.
 *
 * It implements loadr's frozen C-ABI plugin contract (see
 * docs/src/plugins/c-abi.md). The plugin serves the `cecho://` URL scheme and
 * echoes each request back: the response body is the request body verbatim,
 * with HTTP-style status 200.
 *
 * Build (Linux):   make            -> libloadr_plugin_cecho.so
 * Build (macOS):   cc -dynamiclib  -> libloadr_plugin_cecho.dylib
 * Build (Windows): cl /LD          -> loadr_plugin_cecho.dll
 *
 * The JSON shapes match loadr_plugin_api::native::{FfiRequest, FfiResponse}.
 * To stay dependency-free this plugin does *minimal* JSON handling: it pulls
 * the `body_b64` and `method` string fields out of the request with a small
 * scanner (sufficient for an echo) and emits a well-formed response object.
 * A real plugin would link a proper JSON library.
 */

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The C-ABI version this plugin targets. Must match the host's
 * LOADR_C_ABI_VERSION, or the host refuses to load it. */
#define LOADR_C_ABI_VERSION 1u

/* Export markers so the symbols are visible across platforms. */
#if defined(_WIN32)
#define LOADR_EXPORT __declspec(dllexport)
#else
#define LOADR_EXPORT __attribute__((visibility("default")))
#endif

/* ---- allocator contract ----------------------------------------------- *
 * Everything we hand back to the host is allocated here and freed by the
 * host calling loadr_plugin_free with the same ptr/len. We store the length
 * implicitly via malloc; loadr_plugin_free ignores `len` (malloc tracks the
 * real allocation size) but the host still passes it for ABI symmetry. */

LOADR_EXPORT uint32_t loadr_plugin_abi_version(void) {
    return LOADR_C_ABI_VERSION;
}

LOADR_EXPORT void loadr_plugin_free(uint8_t *ptr, size_t len) {
    (void)len;
    free(ptr);
}

/* Duplicate a NUL-terminated C string into a fresh malloc'd buffer, returning
 * the buffer and writing its byte length (excluding NUL) to *out_len. */
static uint8_t *dup_bytes(const char *s, size_t *out_len) {
    size_t n = strlen(s);
    uint8_t *buf = (uint8_t *)malloc(n);
    if (buf == NULL) {
        if (out_len) *out_len = 0;
        return NULL;
    }
    memcpy(buf, s, n);
    if (out_len) *out_len = n;
    return buf;
}

LOADR_EXPORT uint8_t *loadr_plugin_info(size_t *out_len) {
    static const char *INFO =
        "{"
        "\"name\":\"cecho\","
        "\"version\":\"0.1.0\","
        "\"kind\":\"protocol\","
        "\"description\":\"Echo protocol plugin written in C (C-ABI)\","
        "\"schemes\":[\"cecho\"]"
        "}";
    return dup_bytes(INFO, out_len);
}

/* Find the start of a JSON string value for `"<key>":"..."` within `json`
 * (which is NOT NUL-terminated; `len` bounds it). On success returns a pointer
 * to the first char of the value and writes its length to *val_len; returns
 * NULL if the key/value is absent. Handles only un-escaped string values,
 * which is all the echo needs (base64 has no quotes/backslashes). */
static const char *find_str(const char *json, size_t len, const char *key,
                            size_t *val_len) {
    /* Build the search needle: "key": */
    char needle[64];
    int n = snprintf(needle, sizeof(needle), "\"%s\":", key);
    if (n <= 0 || (size_t)n >= sizeof(needle)) return NULL;

    size_t key_len = (size_t)n;
    if (len < key_len) return NULL;

    for (size_t i = 0; i + key_len <= len; i++) {
        if (memcmp(json + i, needle, key_len) != 0) continue;
        size_t p = i + key_len;
        /* skip optional whitespace */
        while (p < len && (json[p] == ' ' || json[p] == '\t')) p++;
        if (p >= len || json[p] != '"') return NULL; /* not a string value */
        p++;                                          /* opening quote */
        size_t start = p;
        while (p < len && json[p] != '"') p++;
        if (p >= len) return NULL; /* unterminated */
        *val_len = p - start;
        return json + start;
    }
    return NULL;
}

/* Append a JSON-escaped (we only need quote/backslash) copy of [s, s+n) to the
 * buffer at *out, growing the count *pos. Caller guarantees capacity. */
static void append_escaped(char *out, size_t *pos, const char *s, size_t n) {
    for (size_t i = 0; i < n; i++) {
        char c = s[i];
        if (c == '"' || c == '\\') out[(*pos)++] = '\\';
        out[(*pos)++] = c;
    }
}

LOADR_EXPORT uint8_t *loadr_plugin_execute(const uint8_t *req, size_t req_len,
                                           size_t *out_len) {
    const char *json = (const char *)req;

    size_t body_len = 0, method_len = 0;
    const char *body = find_str(json, req_len, "body_b64", &body_len);
    const char *method = find_str(json, req_len, "method", &method_len);
    if (body == NULL) {
        body = "";
        body_len = 0;
    }
    if (method == NULL) {
        method = "GET";
        method_len = 3;
    }

    /* Response template; body_b64 echoes the request body verbatim. The
     * `extras.method` is escaped, body_b64 is base64 (no escaping needed). */
    static const char *PRE =
        "{\"status\":200,\"status_text\":\"OK\","
        "\"headers\":[[\"x-cecho\",\"1\"]],"
        "\"body_b64\":\"";
    static const char *MID = "\",\"duration_ms\":0.0,\"extras\":{\"echoed_by\":\"c-echo\",\"method\":\"";
    static const char *POST = "\"}}";

    size_t cap = strlen(PRE) + body_len + strlen(MID) + (method_len * 2) +
                 strlen(POST) + 1;
    char *out = (char *)malloc(cap);
    if (out == NULL) {
        if (out_len) *out_len = 0;
        return NULL;
    }

    size_t pos = 0;
    size_t pre_len = strlen(PRE);
    memcpy(out + pos, PRE, pre_len);
    pos += pre_len;
    memcpy(out + pos, body, body_len); /* base64: safe to copy raw */
    pos += body_len;
    size_t mid_len = strlen(MID);
    memcpy(out + pos, MID, mid_len);
    pos += mid_len;
    append_escaped(out, &pos, method, method_len);
    size_t post_len = strlen(POST);
    memcpy(out + pos, POST, post_len);
    pos += post_len;

    if (out_len) *out_len = pos;
    return (uint8_t *)out;
}
