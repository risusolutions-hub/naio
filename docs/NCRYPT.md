# ncrypt — modern cryptography (AES-GCM, RSA, Ed25519, …)

High-performance native crypto for Niao. Complements `crypto` (SHA-256/512, HMAC). ~cryptography, PyNaCl, secrets subset.

## Import

```niao
import "ncrypt"
```

Paths `import "std/ncrypt"` and `import "ncrypt"` are equivalent. Flat builtins (`ncrypt_token_bytes`, `ncrypt_aes_gcm_seal`, …) are also available globally after import.

## Quick start

```niao
import "ncrypt"

// CSPRNG (secrets module)
let key = ncrypt.token_bytes(32)
let hex = ncrypt.token_hex(16)

// AEAD — seal/open with random nonce prepended
let sealed = ncrypt.aes_gcm_seal(key, byte_array[72, 101, 108, 108, 111])
let plain = ncrypt.aes_gcm_open(key, sealed)

// Ed25519 signatures
let kp = ncrypt.ed25519_generate()
let sig = kp.sign(byte_array[1, 2, 3])
let ok = kp.verify(byte_array[1, 2, 3], sig)

// Key derivation
let dk = ncrypt.hkdf(master_key, 32, {salt: salt, info: byte_array[0], hash: "sha256"})
let pw_key = ncrypt.pbkdf2("password", salt, 600000, 32, {hash: "sha256"})
```

## CSPRNG & constant-time

| Method | Description |
|--------|-------------|
| `ncrypt.token_bytes(n)` | `n` cryptographically random bytes. |
| `ncrypt.token_hex(n)` | `n` random bytes as `2n` hex characters. |
| `ncrypt.token_urlsafe(n)` | URL-safe base64 (no padding) of `n` random bytes. |
| `ncrypt.compare_digest(a, b)` | Constant-time byte comparison. |

## AEAD (AES-GCM & ChaCha20-Poly1305)

| Method | Description |
|--------|-------------|
| `ncrypt.aes_gcm_encrypt(key, plaintext, opts?)` | Encrypt; returns `ciphertext \|\| tag`. |
| `ncrypt.aes_gcm_decrypt(key, nonce, ciphertext, opts?)` | Decrypt authenticated ciphertext. |
| `ncrypt.aes_gcm_seal(key, plaintext, opts?)` | Encrypt with random nonce prepended. |
| `ncrypt.aes_gcm_open(key, sealed, opts?)` | Decrypt sealed blob from `aes_gcm_seal`. |
| `ncrypt.chacha_encrypt(key, plaintext, opts?)` | ChaCha20-Poly1305 encrypt. |
| `ncrypt.chacha_decrypt(key, nonce, ciphertext, opts?)` | ChaCha20-Poly1305 decrypt. |
| `ncrypt.chacha_seal(key, plaintext, opts?)` | ChaCha seal (auto nonce). |
| `ncrypt.chacha_open(key, sealed, opts?)` | ChaCha open. |
| `ncrypt.parallel_aes_encrypt(blocks, key, opts?)` | Parallel batch AES-GCM encrypt. |
| `ncrypt.parallel_aes_decrypt(blocks, key, opts?)` | Parallel batch AES-GCM decrypt. |

### AEAD options

| Key | Default | Description |
|-----|---------|-------------|
| `key_size` | `"aes256"` | `"aes128"` or `"aes256"` for AES-GCM. |
| `nonce` | random 12 bytes | 12-byte nonce for encrypt (required for parallel batch). |
| `aad` | none | Additional authenticated data byte array. |

Constants: `ncrypt.NONCE_LEN` (12), `ncrypt.TAG_LEN` (16), `ncrypt.MAX_BYTES` (256 MiB).

Cipher names: `ncrypt.ciphers.AES128GCM`, `.AES256GCM`, `.CHACHA20POLY1305`.

## KDF

| Method | Description |
|--------|-------------|
| `ncrypt.hkdf(ikm, length, opts?)` | HKDF extract+expand. |
| `ncrypt.hkdf_extract(ikm, opts?)` | HKDF-Extract only. |
| `ncrypt.hkdf_expand(prk, length, opts?)` | HKDF-Expand only. |
| `ncrypt.pbkdf2(password, salt, iterations, length, opts?)` | PBKDF2-HMAC key derivation. |

Options: `salt`, `info` (byte arrays), `hash` (`"sha256"` or `"sha512"`).

## RSA

| Method | Description |
|--------|-------------|
| `ncrypt.rsa_generate(bits)` | Generate 2048/3072/4096-bit keypair → `{private, public}` handle objects. |
| `ncrypt.rsa_public_from_pem(pem)` | Import SPKI public key. |
| `ncrypt.rsa_private_from_pem(pem)` | Import PKCS#8/PKCS#1 private key. |
| `ncrypt.rsa_encrypt(public, data, opts?)` | Encrypt with public key handle/object. |
| `ncrypt.rsa_decrypt(private, data, opts?)` | Decrypt with private key handle/object. |
| `ncrypt.rsa_sign(private, data, opts?)` | Sign message. |
| `ncrypt.rsa_verify(public, data, signature, opts?)` | Verify signature; returns `bool`. |
| `ncrypt.rsa_max_plaintext(public, opts?)` | Max plaintext bytes for padding. |

Handle methods: `public.encrypt(data, opts?)`, `private.decrypt(data, opts?)`, `private.sign(data, opts?)`, `public.verify(data, sig, opts?)`, `.to_pem()`.

Padding options: `padding` (`"oaep"`, `"oaep-sha512"`, `"pkcs1"`), `hash` (`"sha256"`, `"sha512"`), `sign_padding` (`"pss"`, `"pkcs1"`).

## Ed25519

| Method | Description |
|--------|-------------|
| `ncrypt.ed25519_generate()` | Keypair object with `public_key`, `private_key`, `.sign()`, `.verify()`. |
| `ncrypt.ed25519_from_seed(seed)` | 32-byte seed → keypair object. |
| `ncrypt.ed25519_from_private(bytes)` | 32-byte private key → keypair object. |
| `ncrypt.ed25519_sign(private_key, message)` | Sign with raw private key bytes. |
| `ncrypt.ed25519_verify(public_key, message, signature)` | Verify; returns `bool`. |

## X25519

| Method | Description |
|--------|-------------|
| `ncrypt.x25519_generate()` | `{private_key, public_key}` byte arrays. |
| `ncrypt.x25519_from_private(bytes)` | Derive keypair from 32-byte secret. |
| `ncrypt.x25519_shared(private_key, peer_public_key)` | ECDH shared secret (32 bytes). |

## X.509

| Method | Description |
|--------|-------------|
| `ncrypt.x509_parse(pem_or_der)` | Parse certificate → metadata object. |
| `ncrypt.x509_pem_to_der(input)` | PEM → DER bytes. |
| `ncrypt.x509_fingerprint(input)` | SHA-256 fingerprint hex. |

Parse result fields: `subject`, `issuer`, `serial`, `not_before`, `not_after`, `version`, `is_ca`, `signature_algorithm`, `public_key_algorithm`, `public_key_pem`, `fingerprint_sha256`, `san_dns`, `raw_der`.

Certificate chain validation and trust stores are not included (parse/metadata only).

## Errors

Operations return `{__error: true, code, kind: "ncrypt_error", message}` on failure. Authentication failures (bad GCM tag) use code `3590`.

## See also

- `crypto` — SHA-256/512 and HMAC
- `nbinary` — hex/base64 byte utilities
- `nrand` — PRNGs and sampling (non-crypto)
