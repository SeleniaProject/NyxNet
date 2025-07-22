#![forbid(unsafe_code)]

//! age-encrypted keystore utilities.
//!
//! This module provides helper functions to **persist** long-term secret keys
//! (node identity keys, etc.) to disk using the [age] file encryption format
//! recommended in the Nyx design document (§4.2 *Key Store*).  The keystore is
//! encrypted with a *passphrase* supplied by the caller at runtime and the
//! plaintext is **zeroised** from memory immediately after use.
//!
//! The API intentionally keeps I/O concerns minimal to remain flexible for
//! daemon and CLI use:
//!
//! * [`encrypt_and_store()`] – writes an age-encrypted file containing the     
//!   secret to the given path.
//! * [`load_and_decrypt()`] – decrypts the file and returns the secret wrapped
//!   in [`Zeroizing`] to guarantee memory cleansing on drop.
//!
//! ### Example
//! ```rust,no_run
//! use nyx_crypto::keystore::{encrypt_and_store, load_and_decrypt};
//! use zeroize::Zeroizing;
//!
//! let secret = Zeroizing::new(b"my secret key".to_vec());
//! encrypt_and_store(&secret, "./keys.json.age", "correct horse battery staple").unwrap();
//! let recovered = load_and_decrypt("./keys.json.age", "correct horse battery staple").unwrap();
//! assert_eq!(&*secret, &*recovered);
//! ```
//!
//! The implementation exclusively uses in-memory buffers; no plaintext is ever
//! written to disk.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use age::secrecy::Secret;
use age::{Encryptor, Decryptor};
use age::armor::{ArmoredWriter, ArmoredReader, Format};
use zeroize::Zeroizing;
use thiserror::Error;
use std::time::{SystemTime, Duration};

/// Error type for keystore operations.
#[derive(Debug, Error)]
pub enum KeystoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("age encryption error: {0}")]
    Age(#[from] age::Error),
    #[error("decryption failed (incorrect passphrase or corrupt file)")]
    DecryptFailed,
}

/// Encrypt `secret` with `passphrase` and write to `path` in armored age format.
pub fn encrypt_and_store<P: AsRef<Path>>(secret: &Zeroizing<Vec<u8>>, path: P, passphrase: &str) -> Result<(), KeystoreError> {
    // Ensure parent directory exists.
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }

    // Writer → armor → file.
    let file = fs::File::create(&path)?;
    let armor = ArmoredWriter::wrap_output(file, Format::Age)
        .map_err(|e| KeystoreError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    let mut encryptor = Encryptor::with_user_passphrase(Secret::new(passphrase.to_owned()))
        .expect("always ok").wrap_output(armor)?;

    encryptor.write_all(&*secret)?;
    encryptor.finish()?; // flush + close
    Ok(())
}

/// Load age-encrypted keystore from `path`, decrypt with `passphrase` and return secret.
pub fn load_and_decrypt<P: AsRef<Path>>(path: P, passphrase: &str) -> Result<Zeroizing<Vec<u8>>, KeystoreError> {
    let file = fs::File::open(&path)?;
    let mut armor = ArmoredReader::new(file);
    let decryptor = match Decryptor::new(&mut armor)? {
        Decryptor::Passphrase(d) => d,
        _ => return Err(KeystoreError::DecryptFailed),
    };

    let mut reader = decryptor.decrypt(&Secret::new(passphrase.to_owned()), None)?;
    let mut buf = Zeroizing::new(Vec::new());
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Load secret if file exists and not older than `max_age`. Otherwise generate via callback `gen` and store.
pub fn load_or_rotate<P, F>(path: P, passphrase: &str, max_age: Duration, gen: F) -> Result<Zeroizing<Vec<u8>>, KeystoreError>
where P: AsRef<Path>, F: Fn() -> Zeroizing<Vec<u8>> {
    let p = path.as_ref();
    let meta = fs::metadata(p);
    let need_new = match meta {
        Ok(m) => {
            if let Ok(modt) = m.modified() {
                modt.elapsed().unwrap_or(Duration::from_secs(0)) > max_age
            } else { true }
        }
        Err(_) => true,
    };

    if need_new {
        let secret = gen();
        encrypt_and_store(&secret, p, passphrase)?;
        Ok(secret)
    } else {
        load_and_decrypt(p, passphrase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    fn temp_file(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(name);
        p
    }

    #[test]
    fn roundtrip() {
        let path = temp_file("nyx_keystore_test.age");
        let secret = Zeroizing::new(b"top secret key".to_vec());
        encrypt_and_store(&secret, &path, "hunter2").unwrap();
        let recovered = load_and_decrypt(&path, "hunter2").unwrap();
        assert_eq!(&*secret, &*recovered);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn wrong_passphrase() {
        let path = temp_file("nyx_keystore_test2.age");
        let secret = Zeroizing::new(b"another secret".to_vec());
        encrypt_and_store(&secret, &path, "pass1").unwrap();
        let err = load_and_decrypt(&path, "wrongpass").unwrap_err();
        match err {
            KeystoreError::DecryptFailed | KeystoreError::Age(_) => {},
            _ => panic!("unexpected error type"),
        }
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rotate_by_age() {
        let path = temp_file("nyx_keystore_rotate.age");
        let secret = Zeroizing::new(b"old".to_vec());
        encrypt_and_store(&secret, &path, "pw").unwrap();
        // Set mtime to old (simulate 100 days)
        #[cfg(unix)] {
            use filetime::FileTime;
            use std::time::SystemTime;
            let ft = FileTime::from_system_time(SystemTime::now() - std::time::Duration::from_secs(86400*100));
            filetime::set_file_mtime(&path, ft).unwrap();
        }
        let new = load_or_rotate(&path, "pw", std::time::Duration::from_secs(86400*30), || Zeroizing::new(b"newsecret".to_vec())).unwrap();
        assert_eq!(&*new, b"newsecret");
        fs::remove_file(&path).unwrap();
    }
} 