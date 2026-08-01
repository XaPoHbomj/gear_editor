use base64::Engine;
use rsa::{
    RsaPublicKey,
    pkcs1v15::Pkcs1v15Encrypt,
    traits::PaddingScheme,
};
use std::time::Duration;

const SDK_URL: &str = "http://127.0.0.1:20100/account/ma-passport/api/appLoginByPassword";

/// Modulus of the server's RSA-1024 private key (shared with the game client).
/// The SDK decrypts login params with this key, so we encrypt with its public
/// half (exponent 65537) using PKCS#1 v1.5.
const SERVER_MODULUS: [u8; 128] = [
    0x7c, 0x01, 0x67, 0x63, 0xf5, 0x2f, 0x97, 0xbd, 0x43, 0x7d, 0x8e, 0xa8, 0x56, 0x72, 0xa8, 0x2c,
    0xe3, 0x75, 0xb3, 0x5e, 0x14, 0x89, 0x19, 0x42, 0x10, 0x0e, 0x58, 0x03, 0x63, 0x86, 0xa8, 0xc3,
    0xf4, 0x20, 0x5b, 0xd2, 0xd7, 0x09, 0x63, 0xf9, 0xde, 0x76, 0x37, 0xdb, 0x59, 0x2c, 0x55, 0xcf,
    0x8d, 0x85, 0x65, 0x96, 0x1e, 0x70, 0x5c, 0x1e, 0x80, 0x53, 0xd7, 0xae, 0xc7, 0x6e, 0xe5, 0xa5,
    0xe7, 0x3a, 0x12, 0xdf, 0x29, 0x57, 0x69, 0x44, 0x60, 0xc1, 0x45, 0xa9, 0x56, 0xc0, 0x36, 0x0b,
    0xac, 0x0d, 0x93, 0x68, 0xc9, 0x2a, 0x1e, 0x6b, 0xa8, 0x63, 0x59, 0x7c, 0xba, 0x35, 0x71, 0x28,
    0x2f, 0x33, 0xeb, 0x94, 0x3f, 0xbd, 0xd0, 0x7e, 0x32, 0x0e, 0xaa, 0x54, 0x67, 0x98, 0xa2, 0xdb,
    0x96, 0xbc, 0xb4, 0xb2, 0x27, 0xd9, 0x14, 0xc4, 0xc1, 0xb3, 0x77, 0x8e, 0x7d, 0x8d, 0x82, 0x01,
];

fn public_key() -> Result<RsaPublicKey, String> {
    let n = rsa::BigUint::from_bytes_be(&SERVER_MODULUS);
    let e = rsa::BigUint::from(65537u32);
    RsaPublicKey::new(n, e).map_err(|e| format!("rsa key: {e}"))
}

fn encrypt_field(key: &RsaPublicKey, value: &str) -> Result<String, String> {
    let mut rng = rand::thread_rng();
    let padded = PaddingScheme::encrypt(Pkcs1v15Encrypt, &mut rng, key, value.as_bytes())
        .map_err(|e| format!("rsa encrypt: {e}"))?;
    if padded.len() != 128 {
        return Err(format!("rsa block size: {}", padded.len()));
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(padded))
}

/// Registers a new account by calling the SDK's login endpoint, which
/// auto-creates the account when the username does not exist yet.
/// Returns Ok(true) on success, Ok(false) if the SDK rejected it (e.g. wrong
/// password for an existing account), Err on transport/encryption failure.
pub(crate) async fn register_account(username: &str, password: &str) -> Result<bool, String> {
    let key = public_key()?;
    let account_b64 = encrypt_field(&key, username)?;
    let password_b64 = encrypt_field(&key, password)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    let body = serde_json::json!({
        "account": account_b64,
        "password": password_b64,
    });

    let resp = client
        .post(SDK_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("sdk request: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .unwrap_or_else(|_| String::new());

    if !status.is_success() {
        return Err(format!("sdk http {status}: {text}"));
    }

    let retcode = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("retcode").and_then(|r| r.as_i64()))
        .unwrap_or(-1);

    Ok(retcode == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsa_encrypts_to_128_bytes() {
        let key = public_key().unwrap();
        let ciphertext = encrypt_field(&key, "XaPoHbomj").unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(&ciphertext).unwrap();
        assert_eq!(decoded.len(), 128);
    }
}
