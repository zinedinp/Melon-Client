//! Login-phase encryption (vanilla's `EncryptionRequest`/`EncryptionResponse`).

use anyhow::{Context, Result};
use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Encrypt, PublicKey};
use sha1::{Digest, Sha1};

/// The server's key arrives DER-encoded as a `SubjectPublicKeyInfo`.
pub fn parse_public_key(der: &[u8]) -> Result<PublicKey> {
    PublicKey::from_public_key_der(der).context("server's RSA public key is malformed")
}

/// `N` random bytes from the OS CSPRNG, for the AES key and the verify token.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    use rand::Rng;
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

/// PKCS#1 v1.5 encryption, as `EncryptionResponse` requires for the shared secret and the
/// verify token.
pub fn rsa_encrypt(key: &PublicKey, data: &[u8]) -> Result<Vec<u8>> {
    key.encrypt(&mut rand::rng(), Pkcs1v15Encrypt, data)
        .map_err(|e| anyhow::anyhow!("RSA-encrypting for the server's login key failed: {e}"))
}

/// `Crypt.digestData` / `MinecraftSessionService.joinServer`'s `serverId`
pub fn server_hash(server_id: &str, shared_secret: &[u8], public_key_der: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(server_id.as_bytes());
    hasher.update(shared_secret);
    hasher.update(public_key_der);
    let digest = hasher.finalize();
    num_bigint::BigInt::from_signed_bytes_be(&digest).to_str_radix(16)
}
