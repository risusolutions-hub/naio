//! Native `ncrypt` standard library — modern crypto: AES-GCM, ChaCha20-Poly1305,
//! RSA, Ed25519/X25519, HKDF/PBKDF2, X.509 parse, CSPRNG (~cryptography, pynacl).
//!
//! Import with `import "ncrypt"` (or `import "std/ncrypt"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_ncrypt::{
    aead_decrypt, aead_encrypt, aead_open, aead_seal, compare_digest, ed25519_from_private,
    ed25519_from_seed, ed25519_generate, ed25519_private_bytes, ed25519_public_bytes,
    ed25519_public_from_bytes, ed25519_sign, ed25519_verify, fill_random, hkdf, hkdf_expand,
    hkdf_extract, parallel_aead_decrypt, parallel_aead_encrypt, pbkdf2_derive, rsa_decrypt,
    rsa_encrypt, rsa_generate, rsa_max_plaintext_len, rsa_private_from_pem, rsa_private_to_pem,
    rsa_public_from_pem, rsa_public_to_pem, rsa_sign, rsa_verify, token_bytes, token_hex,
    token_urlsafe,     x25519_from_private, x25519_generate, x25519_private_bytes, x25519_public_bytes,
    x25519_public_from_bytes, x25519_shared, x509_fingerprint_sha256, x509_parse, x509_pem_to_der, AeadCipher,
    Ed25519KeyPair, HashAlg, NcryptError, ParsedCert, RsaHash, RsaPadding, RsaSignPadding,
    MAX_BYTES, MAX_TOKEN_BYTES, NONCE_LEN, TAG_LEN,
};
use niao_parallel::available_threads;
use rsa::{RsaPrivateKey, RsaPublicKey};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3567_NCRYPT_ARITY: u32 = codes::E3586_NCRYPT_ARITY;
const E3568_NCRYPT_ERROR: u32 = codes::E3587_NCRYPT_ERROR;
const E3569_NCRYPT_TYPE: u32 = codes::E3588_NCRYPT_TYPE;
const E3570_NCRYPT_INVALID_HANDLE: u32 = codes::E3589_NCRYPT_INVALID_HANDLE;
const E3571_NCRYPT_AUTH: u32 = codes::E3590_NCRYPT_AUTH;
const E3572_NCRYPT_KEY: u32 = codes::E3591_NCRYPT_KEY;

// ---------------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------------

enum NcryptHandle {
    RsaPrivate(RsaPrivateKey),
    RsaPublic(RsaPublicKey),
    Ed25519(Ed25519KeyPair),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, NcryptHandle>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn register(handle: NcryptHandle) -> i64 {
    let id = new_handle();
    HANDLES.with(|m| m.borrow_mut().insert(id, handle));
    id
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&NcryptHandle) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(h) => Ok(Ok(f(h))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3569_NCRYPT_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3567_NCRYPT_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3567_NCRYPT_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn ncrypt_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3568_NCRYPT_ERROR, "ncrypt_error", msg.into(), span)
}

fn auth_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3571_NCRYPT_AUTH, "ncrypt_error", msg.into(), span)
}

fn key_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3572_NCRYPT_KEY, "ncrypt_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3570_NCRYPT_INVALID_HANDLE,
        "ncrypt_error",
        format!("invalid or closed ncrypt handle {id}"),
        span,
    )
}

fn map_err(span: Span, err: NcryptError) -> ValueRef {
    match &err {
        NcryptError::DecryptFailed(m) => auth_err(span, m.clone()),
        NcryptError::InvalidKey(m) => key_err(span, m.clone()),
        _ => ncrypt_err(span, err.message()),
    }
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] or string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

#[allow(dead_code)]
fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        _ => default,
    }
}

fn str_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &str) -> String {
    let Some(map) = map else {
        return default.to_string();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => s,
        _ => default.to_string(),
    }
}

fn bytes_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<Vec<u8>> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::ByteArray(b)) => Some(b),
        Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}

fn bytes_result(bytes: Vec<u8>) -> ValueRef {
    Value::ByteArray(bytes).ref_cell()
}

fn str_val(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn int_val(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn bool_val(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn handle_id_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Object(map) => match map.get("id") {
            Some(v) => match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(type_err(
                    span,
                    format!("{name}() handle id must be int, got {}", other.type_name()),
                )),
            },
            None => Err(type_err(span, format!("{name}() object missing id field"))),
        },
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects handle object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_object(id: i64, kind: &str, mut fields: HashMap<String, ValueRef>) -> ValueRef {
    fields.insert("id".to_string(), int_val(id));
    fields.insert("kind".to_string(), str_val(kind));
    Value::Object(fields).ref_cell()
}

fn bytes_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<Vec<u8>>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::ByteArray(b) => out.push(b.clone()),
                    Value::String(s) => out.push(s.as_bytes().to_vec()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects byte[][] at argument {}; item {} is {}",
                                idx + 1,
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[][] as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_list_result(blocks: Vec<Vec<u8>>) -> ValueRef {
    Value::Array(blocks.into_iter().map(bytes_result).collect()).ref_cell()
}

fn parse_hash_alg(s: &str, span: Span) -> Result<HashAlg, ValueRef> {
    HashAlg::parse(s).map_err(|e| map_err(span, e))
}

fn parse_aes_cipher(map: Option<&HashMap<String, ValueRef>>) -> AeadCipher {
    match str_field(map, "key_size", "aes256").to_ascii_lowercase().as_str() {
        "aes128" | "aes-128" | "128" => AeadCipher::Aes128Gcm,
        _ => AeadCipher::Aes256Gcm,
    }
}

fn parse_rsa_padding(map: Option<&HashMap<String, ValueRef>>, span: Span) -> Result<RsaPadding, ValueRef> {
    RsaPadding::parse(&str_field(map, "padding", "oaep")).map_err(|e| map_err(span, e))
}

fn parse_rsa_hash(map: Option<&HashMap<String, ValueRef>>, span: Span) -> Result<RsaHash, ValueRef> {
    RsaHash::parse(&str_field(map, "hash", "sha256")).map_err(|e| map_err(span, e))
}

fn parse_rsa_sign_padding(
    map: Option<&HashMap<String, ValueRef>>,
    span: Span,
) -> Result<RsaSignPadding, ValueRef> {
    RsaSignPadding::parse(&str_field(map, "sign_padding", "pss")).map_err(|e| map_err(span, e))
}

fn nonce_from_opts_or_random(map: Option<&HashMap<String, ValueRef>>) -> Vec<u8> {
    if let Some(n) = bytes_field(map, "nonce") {
        return n;
    }
    let mut nonce = vec![0u8; NONCE_LEN];
    fill_random(&mut nonce);
    nonce
}

fn aad_from_opts(map: Option<&HashMap<String, ValueRef>>) -> Option<Vec<u8>> {
    bytes_field(map, "aad")
}

fn parsed_cert_object(cert: ParsedCert) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("subject".into(), str_val(cert.subject));
    map.insert("issuer".into(), str_val(cert.issuer));
    map.insert("serial".into(), str_val(cert.serial));
    map.insert("not_before".into(), int_val(cert.not_before));
    map.insert("not_after".into(), int_val(cert.not_after));
    map.insert("version".into(), int_val(cert.version as i64));
    map.insert("is_ca".into(), bool_val(cert.is_ca));
    map.insert("signature_algorithm".into(), str_val(cert.signature_algorithm));
    map.insert("public_key_algorithm".into(), str_val(cert.public_key_algorithm));
    map.insert("public_key_pem".into(), str_val(cert.public_key_pem));
    map.insert("fingerprint_sha256".into(), str_val(cert.fingerprint_sha256));
    map.insert(
        "san_dns".into(),
        Value::Array(cert.san_dns.into_iter().map(str_val).collect()).ref_cell(),
    );
    map.insert("raw_der".into(), bytes_result(cert.raw_der));
    Value::Object(map).ref_cell()
}

fn rsa_private_object(id: i64) -> ValueRef {
    let mut methods = HashMap::new();
    methods.insert("to_pem".into(), Value::NativeFunction(Rc::new(ncrypt_rsa_private_to_pem_method)).ref_cell());
    methods.insert("decrypt".into(), Value::NativeFunction(Rc::new(ncrypt_rsa_private_decrypt_method)).ref_cell());
    methods.insert("sign".into(), Value::NativeFunction(Rc::new(ncrypt_rsa_private_sign_method)).ref_cell());
    handle_object(id, "rsa_private", methods)
}

fn rsa_public_object(id: i64) -> ValueRef {
    let mut methods = HashMap::new();
    methods.insert("to_pem".into(), Value::NativeFunction(Rc::new(ncrypt_rsa_public_to_pem_method)).ref_cell());
    methods.insert("encrypt".into(), Value::NativeFunction(Rc::new(ncrypt_rsa_public_encrypt_method)).ref_cell());
    methods.insert("verify".into(), Value::NativeFunction(Rc::new(ncrypt_rsa_public_verify_method)).ref_cell());
    handle_object(id, "rsa_public", methods)
}

fn ed25519_keypair_object(id: i64, public_key: Vec<u8>, private_key: Vec<u8>) -> ValueRef {
    let mut fields = HashMap::new();
    fields.insert("public_key".into(), bytes_result(public_key));
    fields.insert("private_key".into(), bytes_result(private_key));
    fields.insert("sign".into(), Value::NativeFunction(Rc::new(ncrypt_ed25519_sign_method)).ref_cell());
    fields.insert("verify".into(), Value::NativeFunction(Rc::new(ncrypt_ed25519_verify_method)).ref_cell());
    handle_object(id, "ed25519_keypair", fields)
}

// ---------------------------------------------------------------------------
// CSPRNG / compare
// ---------------------------------------------------------------------------

// >>> len(ncrypt.token_bytes(16))
// 16
fn ncrypt_token_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_token_bytes", span)?;
    let n = int_arg(args, 0, "ncrypt_token_bytes", span)?;
    if n <= 0 {
        return Ok(ncrypt_err(span, "token length must be > 0"));
    }
    match token_bytes(n as usize) {
        Ok(b) => Ok(bytes_result(b)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.token_hex(8))
// 16
fn ncrypt_token_hex(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_token_hex", span)?;
    let n = int_arg(args, 0, "ncrypt_token_hex", span)?;
    if n <= 0 {
        return Ok(ncrypt_err(span, "token length must be > 0"));
    }
    match token_hex(n as usize) {
        Ok(s) => Ok(str_val(s)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.token_urlsafe(12)) > 0
// true
fn ncrypt_token_urlsafe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_token_urlsafe", span)?;
    let n = int_arg(args, 0, "ncrypt_token_urlsafe", span)?;
    if n <= 0 {
        return Ok(ncrypt_err(span, "token length must be > 0"));
    }
    match token_urlsafe(n as usize) {
        Ok(s) => Ok(str_val(s)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncrypt.compare_digest(byte_array[1, 2], byte_array[1, 2])
// true
fn ncrypt_compare_digest(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncrypt_compare_digest", span)?;
    let a = bytes_arg(args, 0, "ncrypt_compare_digest", span)?;
    let b = bytes_arg(args, 1, "ncrypt_compare_digest", span)?;
    Ok(bool_val(compare_digest(&a, &b)))
}

// ---------------------------------------------------------------------------
// AEAD
// ---------------------------------------------------------------------------

// >>> type(ncrypt.aes_gcm_encrypt(key, pt, {"nonce": nonce}))
// "byte[]"
fn ncrypt_aes_gcm_encrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_aes_gcm_encrypt", span)?;
    let key = bytes_arg(args, 0, "ncrypt_aes_gcm_encrypt", span)?;
    let plaintext = bytes_arg(args, 1, "ncrypt_aes_gcm_encrypt", span)?;
    let map = optional_object_arg(args, 2);
    let cipher = parse_aes_cipher(map.as_ref());
    let nonce = nonce_from_opts_or_random(map.as_ref());
    let aad = aad_from_opts(map.as_ref());
    match aead_encrypt(
        cipher,
        &key,
        &nonce,
        &plaintext,
        aad.as_deref(),
    ) {
        Ok(ct) => Ok(bytes_result(ct)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.aes_gcm_decrypt(key, nonce, ct))
// "byte[]"
fn ncrypt_aes_gcm_decrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ncrypt_aes_gcm_decrypt", span)?;
    let key = bytes_arg(args, 0, "ncrypt_aes_gcm_decrypt", span)?;
    let nonce = bytes_arg(args, 1, "ncrypt_aes_gcm_decrypt", span)?;
    let ciphertext = bytes_arg(args, 2, "ncrypt_aes_gcm_decrypt", span)?;
    let map = optional_object_arg(args, 3);
    let cipher = parse_aes_cipher(map.as_ref());
    let aad = aad_from_opts(map.as_ref());
    match aead_decrypt(cipher, &key, &nonce, &ciphertext, aad.as_deref()) {
        Ok(pt) => Ok(bytes_result(pt)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.aes_gcm_seal(key, pt)) > len(pt)
// true
fn ncrypt_aes_gcm_seal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_aes_gcm_seal", span)?;
    let key = bytes_arg(args, 0, "ncrypt_aes_gcm_seal", span)?;
    let plaintext = bytes_arg(args, 1, "ncrypt_aes_gcm_seal", span)?;
    let map = optional_object_arg(args, 2);
    let cipher = parse_aes_cipher(map.as_ref());
    let aad = aad_from_opts(map.as_ref());
    match aead_seal(cipher, &key, &plaintext, aad.as_deref()) {
        Ok(sealed) => Ok(bytes_result(sealed)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncrypt.aes_gcm_open(key, sealed) == pt
// true
fn ncrypt_aes_gcm_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_aes_gcm_open", span)?;
    let key = bytes_arg(args, 0, "ncrypt_aes_gcm_open", span)?;
    let sealed = bytes_arg(args, 1, "ncrypt_aes_gcm_open", span)?;
    let map = optional_object_arg(args, 2);
    let cipher = parse_aes_cipher(map.as_ref());
    let aad = aad_from_opts(map.as_ref());
    match aead_open(cipher, &key, &sealed, aad.as_deref()) {
        Ok(pt) => Ok(bytes_result(pt)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.chacha_encrypt(key, pt, {"nonce": nonce}))
// "byte[]"
fn ncrypt_chacha_encrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_chacha_encrypt", span)?;
    let key = bytes_arg(args, 0, "ncrypt_chacha_encrypt", span)?;
    let plaintext = bytes_arg(args, 1, "ncrypt_chacha_encrypt", span)?;
    let map = optional_object_arg(args, 2);
    let nonce = nonce_from_opts_or_random(map.as_ref());
    let aad = aad_from_opts(map.as_ref());
    match aead_encrypt(
        AeadCipher::ChaCha20Poly1305,
        &key,
        &nonce,
        &plaintext,
        aad.as_deref(),
    ) {
        Ok(ct) => Ok(bytes_result(ct)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.chacha_decrypt(key, nonce, ct))
// "byte[]"
fn ncrypt_chacha_decrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ncrypt_chacha_decrypt", span)?;
    let key = bytes_arg(args, 0, "ncrypt_chacha_decrypt", span)?;
    let nonce = bytes_arg(args, 1, "ncrypt_chacha_decrypt", span)?;
    let ciphertext = bytes_arg(args, 2, "ncrypt_chacha_decrypt", span)?;
    let map = optional_object_arg(args, 3);
    let aad = aad_from_opts(map.as_ref());
    match aead_decrypt(
        AeadCipher::ChaCha20Poly1305,
        &key,
        &nonce,
        &ciphertext,
        aad.as_deref(),
    ) {
        Ok(pt) => Ok(bytes_result(pt)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.chacha_seal(key, pt)) > 0
// true
fn ncrypt_chacha_seal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_chacha_seal", span)?;
    let key = bytes_arg(args, 0, "ncrypt_chacha_seal", span)?;
    let plaintext = bytes_arg(args, 1, "ncrypt_chacha_seal", span)?;
    let map = optional_object_arg(args, 2);
    let aad = aad_from_opts(map.as_ref());
    match aead_seal(
        AeadCipher::ChaCha20Poly1305,
        &key,
        &plaintext,
        aad.as_deref(),
    ) {
        Ok(sealed) => Ok(bytes_result(sealed)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncrypt.chacha_open(key, sealed) == pt
// true
fn ncrypt_chacha_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_chacha_open", span)?;
    let key = bytes_arg(args, 0, "ncrypt_chacha_open", span)?;
    let sealed = bytes_arg(args, 1, "ncrypt_chacha_open", span)?;
    let map = optional_object_arg(args, 2);
    let aad = aad_from_opts(map.as_ref());
    match aead_open(
        AeadCipher::ChaCha20Poly1305,
        &key,
        &sealed,
        aad.as_deref(),
    ) {
        Ok(pt) => Ok(bytes_result(pt)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// KDF
// ---------------------------------------------------------------------------

// >>> len(ncrypt.hkdf(ikm, 32))
// 32
fn ncrypt_hkdf(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_hkdf", span)?;
    let ikm = bytes_arg(args, 0, "ncrypt_hkdf", span)?;
    let length = int_arg(args, 1, "ncrypt_hkdf", span)?;
    if length <= 0 {
        return Ok(ncrypt_err(span, "HKDF length must be > 0"));
    }
    let map = optional_object_arg(args, 2);
    let salt = bytes_field(map.as_ref(), "salt");
    let info = bytes_field(map.as_ref(), "info");
    let hash = match parse_hash_alg(&str_field(map.as_ref(), "hash", "sha256"), span) {
        Ok(h) => h,
        Err(e) => return Ok(e),
    };
    match hkdf(
        &ikm,
        length as usize,
        salt.as_deref(),
        info.as_deref(),
        hash,
    ) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.hkdf_extract(ikm))
// 32
fn ncrypt_hkdf_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncrypt_hkdf_extract", span)?;
    let ikm = bytes_arg(args, 0, "ncrypt_hkdf_extract", span)?;
    let map = optional_object_arg(args, 1);
    let salt = bytes_field(map.as_ref(), "salt");
    let hash = match parse_hash_alg(&str_field(map.as_ref(), "hash", "sha256"), span) {
        Ok(h) => h,
        Err(e) => return Ok(e),
    };
    match hkdf_extract(&ikm, salt.as_deref(), hash) {
        Ok(prk) => Ok(bytes_result(prk)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.hkdf_expand(prk, 16))
// 16
fn ncrypt_hkdf_expand(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_hkdf_expand", span)?;
    let prk = bytes_arg(args, 0, "ncrypt_hkdf_expand", span)?;
    let length = int_arg(args, 1, "ncrypt_hkdf_expand", span)?;
    if length <= 0 {
        return Ok(ncrypt_err(span, "HKDF length must be > 0"));
    }
    let map = optional_object_arg(args, 2);
    let info = bytes_field(map.as_ref(), "info");
    let hash = match parse_hash_alg(&str_field(map.as_ref(), "hash", "sha256"), span) {
        Ok(h) => h,
        Err(e) => return Ok(e),
    };
    match hkdf_expand(&prk, length as usize, info.as_deref(), hash) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.pbkdf2(password, salt, 1000, 32))
// 32
fn ncrypt_pbkdf2(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 5, "ncrypt_pbkdf2", span)?;
    let password = bytes_arg(args, 0, "ncrypt_pbkdf2", span)?;
    let salt = bytes_arg(args, 1, "ncrypt_pbkdf2", span)?;
    let iterations = int_arg(args, 2, "ncrypt_pbkdf2", span)?;
    let length = int_arg(args, 3, "ncrypt_pbkdf2", span)?;
    if iterations <= 0 {
        return Ok(ncrypt_err(span, "PBKDF2 iterations must be >= 1"));
    }
    if length <= 0 {
        return Ok(ncrypt_err(span, "PBKDF2 length must be > 0"));
    }
    let map = optional_object_arg(args, 4);
    let hash = match parse_hash_alg(&str_field(map.as_ref(), "hash", "sha256"), span) {
        Ok(h) => h,
        Err(e) => return Ok(e),
    };
    match pbkdf2_derive(
        &password,
        &salt,
        iterations as u32,
        length as usize,
        hash,
    ) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// RSA
// ---------------------------------------------------------------------------

// >>> type(ncrypt.rsa_generate(2048).public)
// "object"
fn ncrypt_rsa_generate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_rsa_generate", span)?;
    let bits = int_arg(args, 0, "ncrypt_rsa_generate", span)? as usize;
    match rsa_generate(bits) {
        Ok(pair) => {
            let pub_id = register(NcryptHandle::RsaPublic(pair.public));
            let priv_id = register(NcryptHandle::RsaPrivate(pair.private));
            let mut map = HashMap::new();
            map.insert("private".into(), rsa_private_object(priv_id));
            map.insert("public".into(), rsa_public_object(pub_id));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.rsa_public_from_pem(pem))
// "object"
fn ncrypt_rsa_public_from_pem(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_rsa_public_from_pem", span)?;
    let pem = string_arg(args, 0, "ncrypt_rsa_public_from_pem", span)?;
    match rsa_public_from_pem(&pem) {
        Ok(key) => {
            let id = register(NcryptHandle::RsaPublic(key));
            Ok(rsa_public_object(id))
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.rsa_private_from_pem(pem))
// "object"
fn ncrypt_rsa_private_from_pem(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_rsa_private_from_pem", span)?;
    let pem = string_arg(args, 0, "ncrypt_rsa_private_from_pem", span)?;
    match rsa_private_from_pem(&pem) {
        Ok(key) => {
            let id = register(NcryptHandle::RsaPrivate(key));
            Ok(rsa_private_object(id))
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.rsa_encrypt(pub_handle, data))
// "byte[]"
fn ncrypt_rsa_encrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_rsa_encrypt", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "ncrypt_rsa_encrypt")?;
    let data = bytes_arg(args, 1, "ncrypt_rsa_encrypt", span)?;
    let map = optional_object_arg(args, 2);
    let padding = match parse_rsa_padding(map.as_ref(), span) {
        Ok(p) => p,
        Err(e) => return Ok(e),
    };
    match with_handle(handle_id, span, |h| {
        if let NcryptHandle::RsaPublic(key) = h {
            rsa_encrypt(key, &data, padding)
        } else {
            Err(NcryptError::InvalidArgument(
                "rsa_encrypt() requires an RSA public key handle".into(),
            ))
        }
    })? {
        Ok(Ok(out)) => Ok(bytes_result(out)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> type(ncrypt.rsa_decrypt(priv_handle, data))
// "byte[]"
fn ncrypt_rsa_decrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_rsa_decrypt", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "ncrypt_rsa_decrypt")?;
    let data = bytes_arg(args, 1, "ncrypt_rsa_decrypt", span)?;
    let map = optional_object_arg(args, 2);
    let padding = match parse_rsa_padding(map.as_ref(), span) {
        Ok(p) => p,
        Err(e) => return Ok(e),
    };
    match with_handle(handle_id, span, |h| {
        if let NcryptHandle::RsaPrivate(key) = h {
            rsa_decrypt(key, &data, padding)
        } else {
            Err(NcryptError::InvalidArgument(
                "rsa_decrypt() requires an RSA private key handle".into(),
            ))
        }
    })? {
        Ok(Ok(out)) => Ok(bytes_result(out)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> type(ncrypt.rsa_sign(priv_handle, data))
// "byte[]"
fn ncrypt_rsa_sign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_rsa_sign", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "ncrypt_rsa_sign")?;
    let data = bytes_arg(args, 1, "ncrypt_rsa_sign", span)?;
    let map = optional_object_arg(args, 2);
    let hash = match parse_rsa_hash(map.as_ref(), span) {
        Ok(h) => h,
        Err(e) => return Ok(e),
    };
    let sign_padding = match parse_rsa_sign_padding(map.as_ref(), span) {
        Ok(p) => p,
        Err(e) => return Ok(e),
    };
    match with_handle(handle_id, span, |h| {
        if let NcryptHandle::RsaPrivate(key) = h {
            rsa_sign(key, &data, hash, sign_padding)
        } else {
            Err(NcryptError::InvalidArgument(
                "rsa_sign() requires an RSA private key handle".into(),
            ))
        }
    })? {
        Ok(Ok(sig)) => Ok(bytes_result(sig)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> type(ncrypt.rsa_verify(pub_handle, data, sig))
// "bool"
fn ncrypt_rsa_verify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ncrypt_rsa_verify", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "ncrypt_rsa_verify")?;
    let data = bytes_arg(args, 1, "ncrypt_rsa_verify", span)?;
    let signature = bytes_arg(args, 2, "ncrypt_rsa_verify", span)?;
    let map = optional_object_arg(args, 3);
    let hash = match parse_rsa_hash(map.as_ref(), span) {
        Ok(h) => h,
        Err(e) => return Ok(e),
    };
    let sign_padding = match parse_rsa_sign_padding(map.as_ref(), span) {
        Ok(p) => p,
        Err(e) => return Ok(e),
    };
    match with_handle(handle_id, span, |h| {
        if let NcryptHandle::RsaPublic(key) = h {
            rsa_verify(key, &data, &signature, hash, sign_padding)
        } else {
            Err(NcryptError::InvalidArgument(
                "rsa_verify() requires an RSA public key handle".into(),
            ))
        }
    })? {
        Ok(Ok(ok)) => Ok(bool_val(ok)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> ncrypt.rsa_max_plaintext(pub_handle) > 0
// true
fn ncrypt_rsa_max_plaintext(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncrypt_rsa_max_plaintext", span)?;
    let handle_id = handle_id_from_arg(args, 0, span, "ncrypt_rsa_max_plaintext")?;
    let map = optional_object_arg(args, 1);
    let padding = match parse_rsa_padding(map.as_ref(), span) {
        Ok(p) => p,
        Err(e) => return Ok(e),
    };
    match with_handle(handle_id, span, |h| {
        if let NcryptHandle::RsaPublic(key) = h {
            rsa_max_plaintext_len(key, padding)
        } else {
            Err(NcryptError::InvalidArgument(
                "rsa_max_plaintext() requires an RSA public key handle".into(),
            ))
        }
    })? {
        Ok(Ok(n)) => Ok(int_val(n as i64)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn ncrypt_rsa_private_to_pem_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "rsa_private.to_pem", span)?;
    let id = handle_id_from_arg(args, 0, span, "rsa_private.to_pem")?;
    match with_handle(id, span, |h| {
        if let NcryptHandle::RsaPrivate(key) = h {
            rsa_private_to_pem(key)
        } else {
            Err(NcryptError::InvalidArgument("invalid RSA private handle".into()))
        }
    })? {
        Ok(Ok(pem)) => Ok(str_val(pem)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn ncrypt_rsa_public_to_pem_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "rsa_public.to_pem", span)?;
    let id = handle_id_from_arg(args, 0, span, "rsa_public.to_pem")?;
    match with_handle(id, span, |h| {
        if let NcryptHandle::RsaPublic(key) = h {
            rsa_public_to_pem(key)
        } else {
            Err(NcryptError::InvalidArgument("invalid RSA public handle".into()))
        }
    })? {
        Ok(Ok(pem)) => Ok(str_val(pem)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn ncrypt_rsa_private_decrypt_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "rsa_private.decrypt", span)?;
    ncrypt_rsa_decrypt(args, span)
}

fn ncrypt_rsa_private_sign_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "rsa_private.sign", span)?;
    ncrypt_rsa_sign(args, span)
}

fn ncrypt_rsa_public_encrypt_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "rsa_public.encrypt", span)?;
    ncrypt_rsa_encrypt(args, span)
}

fn ncrypt_rsa_public_verify_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "rsa_public.verify", span)?;
    ncrypt_rsa_verify(args, span)
}

// ---------------------------------------------------------------------------
// Ed25519
// ---------------------------------------------------------------------------

// >>> type(ncrypt.ed25519_generate())
// "object"
fn ncrypt_ed25519_generate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncrypt_ed25519_generate", span)?;
    let _ = args;
    match ed25519_generate() {
        Ok(pair) => {
            let public_key = ed25519_public_bytes(&pair.verifying);
            let private_key = ed25519_private_bytes(&pair);
            let id = register(NcryptHandle::Ed25519(pair));
            Ok(ed25519_keypair_object(id, public_key, private_key))
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.ed25519_from_seed(seed))
// "object"
fn ncrypt_ed25519_from_seed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_ed25519_from_seed", span)?;
    let seed = bytes_arg(args, 0, "ncrypt_ed25519_from_seed", span)?;
    match ed25519_from_seed(&seed) {
        Ok(pair) => {
            let public_key = ed25519_public_bytes(&pair.verifying);
            let private_key = ed25519_private_bytes(&pair);
            let id = register(NcryptHandle::Ed25519(pair));
            Ok(ed25519_keypair_object(id, public_key, private_key))
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.ed25519_from_private(sk))
// "object"
fn ncrypt_ed25519_from_private(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_ed25519_from_private", span)?;
    let sk = bytes_arg(args, 0, "ncrypt_ed25519_from_private", span)?;
    match ed25519_from_private(&sk) {
        Ok(pair) => {
            let public_key = ed25519_public_bytes(&pair.verifying);
            let private_key = ed25519_private_bytes(&pair);
            let id = register(NcryptHandle::Ed25519(pair));
            Ok(ed25519_keypair_object(id, public_key, private_key))
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.ed25519_sign(private_key, msg))
// 64
fn ncrypt_ed25519_sign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncrypt_ed25519_sign", span)?;
    let private_key = bytes_arg(args, 0, "ncrypt_ed25519_sign", span)?;
    let message = bytes_arg(args, 1, "ncrypt_ed25519_sign", span)?;
    match ed25519_from_private(&private_key) {
        Ok(pair) => match ed25519_sign(&pair, &message) {
            Ok(sig) => Ok(bytes_result(sig)),
            Err(e) => Ok(map_err(span, e)),
        },
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.ed25519_verify(public_key, msg, sig))
// "bool"
fn ncrypt_ed25519_verify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncrypt_ed25519_verify", span)?;
    let public_key = bytes_arg(args, 0, "ncrypt_ed25519_verify", span)?;
    let message = bytes_arg(args, 1, "ncrypt_ed25519_verify", span)?;
    let signature = bytes_arg(args, 2, "ncrypt_ed25519_verify", span)?;
    match ed25519_public_from_bytes(&public_key) {
        Ok(pk) => match ed25519_verify(&pk, &message, &signature) {
            Ok(ok) => Ok(bool_val(ok)),
            Err(e) => Ok(map_err(span, e)),
        },
        Err(e) => Ok(map_err(span, e)),
    }
}

fn ncrypt_ed25519_sign_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ed25519_keypair.sign", span)?;
    let id = handle_id_from_arg(args, 0, span, "ed25519_keypair.sign")?;
    let message = bytes_arg(args, 1, "ed25519_keypair.sign", span)?;
    match with_handle(id, span, |h| {
        if let NcryptHandle::Ed25519(pair) = h {
            ed25519_sign(pair, &message)
        } else {
            Err(NcryptError::InvalidArgument("invalid Ed25519 handle".into()))
        }
    })? {
        Ok(Ok(sig)) => Ok(bytes_result(sig)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn ncrypt_ed25519_verify_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ed25519_keypair.verify", span)?;
    let id = handle_id_from_arg(args, 0, span, "ed25519_keypair.verify")?;
    let message = bytes_arg(args, 1, "ed25519_keypair.verify", span)?;
    let signature = bytes_arg(args, 2, "ed25519_keypair.verify", span)?;
    match with_handle(id, span, |h| {
        if let NcryptHandle::Ed25519(pair) = h {
            ed25519_verify(&pair.verifying, &message, &signature)
        } else {
            Err(NcryptError::InvalidArgument("invalid Ed25519 handle".into()))
        }
    })? {
        Ok(Ok(ok)) => Ok(bool_val(ok)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// X25519
// ---------------------------------------------------------------------------

// >>> type(ncrypt.x25519_generate().public_key)
// "byte[]"
fn ncrypt_x25519_generate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncrypt_x25519_generate", span)?;
    let _ = args;
    match x25519_generate() {
        Ok(pair) => {
            let mut map = HashMap::new();
            map.insert("private_key".into(), bytes_result(x25519_private_bytes(&pair)));
            map.insert("public_key".into(), bytes_result(x25519_public_bytes(&pair.public)));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.x25519_shared(priv, peer_pub))
// 32
fn ncrypt_x25519_shared(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncrypt_x25519_shared", span)?;
    let private_key = bytes_arg(args, 0, "ncrypt_x25519_shared", span)?;
    let peer_public = bytes_arg(args, 1, "ncrypt_x25519_shared", span)?;
    match x25519_from_private(&private_key) {
        Ok(pair) => match x25519_public_from_bytes(&peer_public) {
            Ok(peer) => Ok(bytes_result(x25519_shared(&pair.secret, &peer))),
            Err(e) => Ok(map_err(span, e)),
        },
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.x25519_from_private(sk).public_key)
// "byte[]"
fn ncrypt_x25519_from_private(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_x25519_from_private", span)?;
    let sk = bytes_arg(args, 0, "ncrypt_x25519_from_private", span)?;
    match x25519_from_private(&sk) {
        Ok(pair) => {
            let mut map = HashMap::new();
            map.insert("private_key".into(), bytes_result(x25519_private_bytes(&pair)));
            map.insert("public_key".into(), bytes_result(x25519_public_bytes(&pair.public)));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// X509
// ---------------------------------------------------------------------------

// >>> type(ncrypt.x509_parse(pem))
// "object"
fn ncrypt_x509_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_x509_parse", span)?;
    let input = bytes_arg(args, 0, "ncrypt_x509_parse", span)?;
    match x509_parse(&input) {
        Ok(cert) => Ok(parsed_cert_object(cert)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.x509_pem_to_der(pem)) > 0
// true
fn ncrypt_x509_pem_to_der(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_x509_pem_to_der", span)?;
    let input = string_arg(args, 0, "ncrypt_x509_pem_to_der", span)?;
    match x509_pem_to_der(&input) {
        Ok(der) => Ok(bytes_result(der)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> len(ncrypt.x509_fingerprint(cert_bytes))
// 64
fn ncrypt_x509_fingerprint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncrypt_x509_fingerprint", span)?;
    let input = bytes_arg(args, 0, "ncrypt_x509_fingerprint", span)?;
    match x509_fingerprint_sha256(&input) {
        Ok(fp) => Ok(str_val(fp)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Parallel AES
// ---------------------------------------------------------------------------

// >>> type(ncrypt.parallel_aes_encrypt(blocks, key, {"nonce": nonce}))
// "array"
fn ncrypt_parallel_aes_encrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_parallel_aes_encrypt", span)?;
    let blocks = bytes_list_arg(args, 0, "ncrypt_parallel_aes_encrypt", span)?;
    let key = bytes_arg(args, 1, "ncrypt_parallel_aes_encrypt", span)?;
    let map = optional_object_arg(args, 2);
    let cipher = parse_aes_cipher(map.as_ref());
    let nonce = nonce_from_opts_or_random(map.as_ref());
    let threads = int_field(map.as_ref(), "threads", available_threads() as i64).max(1) as usize;
    match parallel_aead_encrypt(&blocks, &key, &nonce, cipher, threads) {
        Ok(out) => Ok(bytes_list_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(ncrypt.parallel_aes_decrypt(blocks, key, {"nonce": nonce}))
// "array"
fn ncrypt_parallel_aes_decrypt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncrypt_parallel_aes_decrypt", span)?;
    let blocks = bytes_list_arg(args, 0, "ncrypt_parallel_aes_decrypt", span)?;
    let key = bytes_arg(args, 1, "ncrypt_parallel_aes_decrypt", span)?;
    let map = optional_object_arg(args, 2);
    let cipher = parse_aes_cipher(map.as_ref());
    let nonce = nonce_from_opts_or_random(map.as_ref());
    let threads = int_field(map.as_ref(), "threads", available_threads() as i64).max(1) as usize;
    match parallel_aead_decrypt(&blocks, &key, &nonce, cipher, threads) {
        Ok(out) => Ok(bytes_list_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! ncrypt_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncrypt_fns![
    ("ncrypt_token_bytes", "token_bytes", ncrypt_token_bytes),
    ("ncrypt_token_hex", "token_hex", ncrypt_token_hex),
    ("ncrypt_token_urlsafe", "token_urlsafe", ncrypt_token_urlsafe),
    ("ncrypt_compare_digest", "compare_digest", ncrypt_compare_digest),
    ("ncrypt_aes_gcm_encrypt", "aes_gcm_encrypt", ncrypt_aes_gcm_encrypt),
    ("ncrypt_aes_gcm_decrypt", "aes_gcm_decrypt", ncrypt_aes_gcm_decrypt),
    ("ncrypt_aes_gcm_seal", "aes_gcm_seal", ncrypt_aes_gcm_seal),
    ("ncrypt_aes_gcm_open", "aes_gcm_open", ncrypt_aes_gcm_open),
    ("ncrypt_chacha_encrypt", "chacha_encrypt", ncrypt_chacha_encrypt),
    ("ncrypt_chacha_decrypt", "chacha_decrypt", ncrypt_chacha_decrypt),
    ("ncrypt_chacha_seal", "chacha_seal", ncrypt_chacha_seal),
    ("ncrypt_chacha_open", "chacha_open", ncrypt_chacha_open),
    ("ncrypt_hkdf", "hkdf", ncrypt_hkdf),
    ("ncrypt_hkdf_extract", "hkdf_extract", ncrypt_hkdf_extract),
    ("ncrypt_hkdf_expand", "hkdf_expand", ncrypt_hkdf_expand),
    ("ncrypt_pbkdf2", "pbkdf2", ncrypt_pbkdf2),
    ("ncrypt_rsa_generate", "rsa_generate", ncrypt_rsa_generate),
    ("ncrypt_rsa_public_from_pem", "rsa_public_from_pem", ncrypt_rsa_public_from_pem),
    ("ncrypt_rsa_private_from_pem", "rsa_private_from_pem", ncrypt_rsa_private_from_pem),
    ("ncrypt_rsa_encrypt", "rsa_encrypt", ncrypt_rsa_encrypt),
    ("ncrypt_rsa_decrypt", "rsa_decrypt", ncrypt_rsa_decrypt),
    ("ncrypt_rsa_sign", "rsa_sign", ncrypt_rsa_sign),
    ("ncrypt_rsa_verify", "rsa_verify", ncrypt_rsa_verify),
    ("ncrypt_rsa_max_plaintext", "rsa_max_plaintext", ncrypt_rsa_max_plaintext),
    ("ncrypt_ed25519_generate", "ed25519_generate", ncrypt_ed25519_generate),
    ("ncrypt_ed25519_from_seed", "ed25519_from_seed", ncrypt_ed25519_from_seed),
    ("ncrypt_ed25519_from_private", "ed25519_from_private", ncrypt_ed25519_from_private),
    ("ncrypt_ed25519_sign", "ed25519_sign", ncrypt_ed25519_sign),
    ("ncrypt_ed25519_verify", "ed25519_verify", ncrypt_ed25519_verify),
    ("ncrypt_x25519_generate", "x25519_generate", ncrypt_x25519_generate),
    ("ncrypt_x25519_shared", "x25519_shared", ncrypt_x25519_shared),
    ("ncrypt_x25519_from_private", "x25519_from_private", ncrypt_x25519_from_private),
    ("ncrypt_x509_parse", "x509_parse", ncrypt_x509_parse),
    ("ncrypt_x509_pem_to_der", "x509_pem_to_der", ncrypt_x509_pem_to_der),
    ("ncrypt_x509_fingerprint", "x509_fingerprint", ncrypt_x509_fingerprint),
    ("ncrypt_parallel_aes_encrypt", "parallel_aes_encrypt", ncrypt_parallel_aes_encrypt),
    ("ncrypt_parallel_aes_decrypt", "parallel_aes_decrypt", ncrypt_parallel_aes_decrypt),
];

pub const MODULE_NAME: &str = "ncrypt";
pub const MODULE_PATHS: &[&str] = &["ncrypt", "std/ncrypt"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    let mut ciphers = HashMap::new();
    for c in [
        AeadCipher::Aes128Gcm,
        AeadCipher::Aes256Gcm,
        AeadCipher::ChaCha20Poly1305,
    ] {
        ciphers.insert(
            c.as_str().to_uppercase().replace('-', "_"),
            str_val(c.as_str()),
        );
    }
    map.insert("ciphers".into(), Value::Object(ciphers).ref_cell());
    map.insert("NONCE_LEN".into(), int_val(NONCE_LEN as i64));
    map.insert("TAG_LEN".into(), int_val(TAG_LEN as i64));
    map.insert("MAX_BYTES".into(), int_val(MAX_BYTES as i64));
    map.insert("MAX_TOKEN_BYTES".into(), int_val(MAX_TOKEN_BYTES as i64));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn token_bytes_doctest() {
        let out = ncrypt_token_bytes(&[Value::Int(16).ref_cell()], span()).unwrap();
        match &*out.borrow() {
            Value::ByteArray(b) => assert_eq!(b.len(), 16),
            other => panic!("expected bytes, got {other:?}"),
        }
    }

    #[test]
    fn aes_gcm_roundtrip() {
        let key = vec![7u8; 32];
        let nonce = vec![1u8; 12];
        let pt = b"hello ncrypt".to_vec();
        let mut opts = HashMap::new();
        opts.insert("nonce".into(), bytes_result(nonce.clone()));
        let ct = ncrypt_aes_gcm_encrypt(
            &[
                bytes_result(key.clone()),
                bytes_result(pt.clone()),
                Value::Object(opts).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let back = ncrypt_aes_gcm_decrypt(
            &[
                bytes_result(key),
                bytes_result(nonce),
                ct,
            ],
            span(),
        )
        .unwrap();
        match &*back.borrow() {
            Value::ByteArray(b) => assert_eq!(b, &pt),
            other => panic!("expected bytes, got {other:?}"),
        }
    }

    #[test]
    fn ed25519_sign_verify() {
        let pair = ncrypt_ed25519_generate(&[], span()).unwrap();
        let handle = match &*pair.borrow() {
            Value::Object(map) => map["id"].clone(),
            other => panic!("expected object, got {other:?}"),
        };
        let sig = ncrypt_ed25519_sign_method(
            &[handle.clone(), bytes_result(b"msg".to_vec())],
            span(),
        )
        .unwrap();
        let ok = ncrypt_ed25519_verify_method(
            &[handle, bytes_result(b"msg".to_vec()), sig],
            span(),
        )
        .unwrap();
        match &*ok.borrow() {
            Value::Bool(true) => {}
            other => panic!("expected true, got {other:?}"),
        }
    }
}
