//! Micro-benchmark for ncrypt hot paths.
use niao_ncrypt::{
    aead_decrypt, aead_encrypt, aead_seal, compare_digest, ed25519_generate, ed25519_sign,
    ed25519_verify, hkdf, token_bytes, AeadCipher, HashAlg,
};
use std::time::Instant;

fn bench<F: FnMut()>(name: &str, iters: usize, mut f: F) {
    for _ in 0..10 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let ns = start.elapsed().as_nanos() as f64 / iters as f64;
    println!("{name:32} {ns:12.0} ns/op");
}

fn main() {
    let iters = 200;
    let key = vec![0xABu8; 32];
    let nonce = [1u8; 12];
    let payload = vec![0xCDu8; 64 * 1024];

    println!(
        "=== ncrypt bench (payload {} KiB, {} iters) ===",
        payload.len() / 1024,
        iters
    );

    bench("aes256-gcm encrypt", iters, || {
        let _ = aead_encrypt(AeadCipher::Aes256Gcm, &key, &nonce, &payload, None).unwrap();
    });
    let ct = aead_encrypt(AeadCipher::Aes256Gcm, &key, &nonce, &payload, None).unwrap();
    bench("aes256-gcm decrypt", iters, || {
        let _ = aead_decrypt(AeadCipher::Aes256Gcm, &key, &nonce, &ct, None).unwrap();
    });

    bench("chacha20-poly1305 encrypt", iters, || {
        let _ = aead_encrypt(AeadCipher::ChaCha20Poly1305, &key, &nonce, &payload, None).unwrap();
    });
    let c2 = aead_encrypt(AeadCipher::ChaCha20Poly1305, &key, &nonce, &payload, None).unwrap();
    bench("chacha20-poly1305 decrypt", iters, || {
        let _ = aead_decrypt(AeadCipher::ChaCha20Poly1305, &key, &nonce, &c2, None).unwrap();
    });

    bench("aes256-gcm seal (auto nonce)", iters, || {
        let _ = aead_seal(AeadCipher::Aes256Gcm, &key, &payload, None).unwrap();
    });

    bench("token_bytes(32)", iters * 10, || {
        let _ = token_bytes(32).unwrap();
    });

    bench("hkdf derive 32", iters * 10, || {
        let _ = hkdf(b"ikm", 32, Some(b"salt"), Some(b"info"), HashAlg::Sha256).unwrap();
    });

    let ed = ed25519_generate().unwrap();
    bench("ed25519 sign", iters * 10, || {
        let _ = ed25519_sign(&ed, b"benchmark message").unwrap();
    });
    let sig = ed25519_sign(&ed, b"benchmark message").unwrap();
    bench("ed25519 verify", iters * 10, || {
        let _ = ed25519_verify(&ed.verifying, b"benchmark message", &sig).unwrap();
    });

    let a = token_bytes(32).unwrap();
    let b = token_bytes(32).unwrap();
    bench("compare_digest", iters * 20, || {
        let _ = compare_digest(&a, &b);
    });
}
