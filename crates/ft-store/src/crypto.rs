//! Credential encryption.
//!
//! The Flutter app stored bot passwords as plaintext in SharedPreferences.
//! Here they are AES-256-GCM sealed, with the key held outside the database so
//! that copying the `.db` file off a device yields nothing usable.
//!
//! The key comes from a [`KeyProvider`]. On desktop that is a 0600 file beside
//! the database; on Android it will be an Android Keystore-wrapped key (M12).
//! AES-GCM is used rather than SQLCipher deliberately: SQLCipher would pull
//! OpenSSL into the NDK build, which is a cross-compilation problem we do not
//! need (see docs/android-notes.md).

use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};

use crate::error::{Result, StoreError};

/// AES-256 key length.
pub const KEY_LEN: usize = 32;
/// GCM nonce length.
const NONCE_LEN: usize = 12;

/// Supplies the encryption key.
///
/// Implemented per platform so the store itself does not care where the key
/// lives.
pub trait KeyProvider: Send + Sync {
    /// Returns the key, creating one on first use.
    fn key(&self) -> Result<[u8; KEY_LEN]>;
}

/// Key held in a 0600 file, created on first use. The desktop implementation.
#[derive(Debug, Clone)]
pub struct FileKey {
    pub(crate) path: PathBuf,
}

impl FileKey {
    /// Uses `<db-path>.key`, so the key sits beside the database but never in it.
    pub fn beside(db_path: &Path) -> Self {
        let mut path = db_path.as_os_str().to_owned();
        path.push(".key");
        Self {
            path: PathBuf::from(path),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl KeyProvider for FileKey {
    fn key(&self) -> Result<[u8; KEY_LEN]> {
        if let Ok(existing) = std::fs::read(&self.path) {
            if existing.len() == KEY_LEN {
                let mut key = [0u8; KEY_LEN];
                key.copy_from_slice(&existing);
                return Ok(key);
            }
            // Wrong length means a truncated or corrupt file. Refuse rather
            // than silently regenerating, which would strand every saved
            // password as undecryptable.
            return Err(StoreError::Key(format!(
                "{} is {} bytes, expected {KEY_LEN}",
                self.path.display(),
                existing.len()
            )));
        }

        let key = random_key()?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StoreError::Key(e.to_string()))?;
        }
        write_private(&self.path, &key)?;
        Ok(key)
    }
}

/// Writes owner-read-only where the platform supports it.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| StoreError::Key(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| StoreError::Key(e.to_string()))?;
    }
    Ok(())
}

fn random_key() -> Result<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    getrandom::fill(&mut key).map_err(|e| StoreError::Key(e.to_string()))?;
    Ok(key)
}

/// An in-memory key, for tests.
#[derive(Debug, Clone)]
pub struct StaticKey(pub [u8; KEY_LEN]);

impl StaticKey {
    pub fn random() -> Result<Self> {
        Ok(Self(random_key()?))
    }
}

impl KeyProvider for StaticKey {
    fn key(&self) -> Result<[u8; KEY_LEN]> {
        Ok(self.0)
    }
}

/// Seals and opens secrets with a key from a [`KeyProvider`].
pub struct Crypto {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for Crypto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Crypto(<key elided>)")
    }
}

impl Crypto {
    pub fn new(provider: &dyn KeyProvider) -> Result<Self> {
        let key: Key<Aes256Gcm> = provider.key()?.into();
        Ok(Self {
            cipher: Aes256Gcm::new(&key),
        })
    }

    /// Encrypts, returning `(nonce, ciphertext)`.
    ///
    /// A fresh random nonce per call: reusing one under the same key would let
    /// an observer recover the XOR of two passwords.
    pub fn seal(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce_bytes).map_err(|e| StoreError::Crypto(e.to_string()))?;
        let nonce: Nonce<_> = nonce_bytes.into();
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| StoreError::Crypto("encryption failed".into()))?;
        Ok((nonce_bytes.to_vec(), ciphertext))
    }

    /// Decrypts. Fails if the key has changed or the row was tampered with;
    /// GCM authenticates, so a flipped bit is an error rather than garbage.
    pub fn open(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<String> {
        let nonce: Nonce<_> = <[u8; NONCE_LEN]>::try_from(nonce)
            .map_err(|_| {
                StoreError::Crypto(format!(
                    "nonce is {} bytes, expected {NONCE_LEN}",
                    nonce.len()
                ))
            })?
            .into();
        let plaintext = self.cipher.decrypt(&nonce, ciphertext).map_err(|_| {
            StoreError::Crypto(
                "could not decrypt the stored password -- the key file may have been \
                     replaced; re-add the bot"
                    .into(),
            )
        })?;
        String::from_utf8(plaintext).map_err(|e| StoreError::Crypto(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_password() {
        let crypto = Crypto::new(&StaticKey::random().unwrap()).unwrap();
        let (nonce, ct) = crypto.seal("hunter2").unwrap();
        assert_eq!(crypto.open(&nonce, &ct).unwrap(), "hunter2");
    }

    #[test]
    fn ciphertext_does_not_contain_the_plaintext() {
        let crypto = Crypto::new(&StaticKey::random().unwrap()).unwrap();
        let (_, ct) = crypto.seal("hunter2").unwrap();
        assert!(!ct.windows(7).any(|w| w == b"hunter2"));
    }

    #[test]
    fn the_same_password_seals_differently_each_time() {
        // A fixed nonce would leak that two bots share a password, and worse.
        let crypto = Crypto::new(&StaticKey::random().unwrap()).unwrap();
        let (n1, c1) = crypto.seal("same").unwrap();
        let (n2, c2) = crypto.seal("same").unwrap();
        assert_ne!(n1, n2);
        assert_ne!(c1, c2);
        assert_eq!(crypto.open(&n1, &c1).unwrap(), "same");
        assert_eq!(crypto.open(&n2, &c2).unwrap(), "same");
    }

    #[test]
    fn a_different_key_cannot_open_it() {
        let a = Crypto::new(&StaticKey::random().unwrap()).unwrap();
        let b = Crypto::new(&StaticKey::random().unwrap()).unwrap();
        let (nonce, ct) = a.seal("hunter2").unwrap();
        let err = b.open(&nonce, &ct).unwrap_err();
        assert!(err.to_string().contains("re-add the bot"));
    }

    #[test]
    fn tampering_is_detected() {
        // GCM authenticates, so a flipped bit must error rather than decrypt to
        // garbage that we would then send to the bot as a password.
        let crypto = Crypto::new(&StaticKey::random().unwrap()).unwrap();
        let (nonce, mut ct) = crypto.seal("hunter2").unwrap();
        ct[0] ^= 0x01;
        assert!(crypto.open(&nonce, &ct).is_err());
    }

    #[test]
    fn a_file_key_persists_and_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let provider = FileKey::beside(&path);

        let first = provider.key().unwrap();
        let second = provider.key().unwrap();
        assert_eq!(first, second, "key must be stable across calls");

        let key_path = dir.path().join("test.db.key");
        assert!(key_path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key_path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "key file is world-readable");
        }
    }

    #[test]
    fn the_key_file_sits_beside_the_database_with_a_known_name() {
        // Android migrates an existing plaintext key into the platform
        // keystore by looking for exactly this filename. Renaming it here
        // without updating that lookup would silently generate a fresh key on
        // upgrade and strand every saved credential.
        let path = std::path::Path::new("/data/user/0/app/files/ft.db");
        assert_eq!(
            FileKey::beside(path).path,
            std::path::PathBuf::from("/data/user/0/app/files/ft.db.key")
        );
    }

    #[test]
    fn a_truncated_key_file_is_refused_not_regenerated() {
        // Silently regenerating would strand every saved password as
        // undecryptable, with no indication why.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.key");
        std::fs::write(&path, b"too short").unwrap();
        let err = FileKey::at(&path).key().unwrap_err();
        assert!(err.to_string().contains("expected 32"));
    }
}
