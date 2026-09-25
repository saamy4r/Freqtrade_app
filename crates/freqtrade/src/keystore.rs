//! Credential key storage backed by the Android Keystore.
//!
//! `ft-store` encrypts bot passwords with a 32-byte key that has to live
//! somewhere. On desktop that is a 0600 file. On Android the same file sits in
//! app-private storage, which is already out of reach of other apps — but it
//! is plainly readable to anyone with root, and to anyone who can pull the
//! app's data directory.
//!
//! So the key itself is wrapped by a second key held in the Android Keystore.
//! That one is generated inside the platform's keystore, is marked
//! non-exportable, and on most modern phones lives in a secure element the
//! operating system cannot read either. What lands on disk is only the
//! ciphertext.
//!
//! An existing plaintext key is migrated in place the first time this runs, so
//! upgrading does not lose saved credentials.

#![cfg(target_os = "android")]

use std::path::{Path, PathBuf};

use ft_store::{KeyProvider, StoreError, KEY_LEN};
use jni::objects::{JByteArray, JObject, JValue};
use jni::JNIEnv;

/// Alias of the wrapping key inside the platform keystore.
const ALIAS: &str = "freqtrade-db-key";

/// `KeyProperties.PURPOSE_ENCRYPT | PURPOSE_DECRYPT`.
const PURPOSES: i32 = 1 | 2;
/// `Cipher.ENCRYPT_MODE` / `Cipher.DECRYPT_MODE`.
const ENCRYPT_MODE: i32 = 1;
const DECRYPT_MODE: i32 = 2;
/// GCM tag length in bits.
const TAG_BITS: i32 = 128;
/// GCM nonce length, which the platform chooses for us on encrypt.
const IV_LEN: usize = 12;

/// Wraps the database key with a non-exportable key held by the platform.
pub struct AndroidKeystore {
    /// Where the wrapped key is stored: `<data dir>/db.key.enc`.
    wrapped: PathBuf,
    /// The plaintext key written by earlier versions, migrated then deleted.
    legacy: PathBuf,
}

impl AndroidKeystore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            wrapped: data_dir.join("db.key.enc"),
            legacy: data_dir.join("ft.db.key"),
        }
    }
}

impl KeyProvider for AndroidKeystore {
    fn key(&self) -> Result<[u8; KEY_LEN], StoreError> {
        let err = |e: String| StoreError::Key(e);

        if self.wrapped.exists() {
            let blob = std::fs::read(&self.wrapped).map_err(|e| err(e.to_string()))?;
            return unwrap_key(&blob).map_err(err);
        }

        // Carry an existing plaintext key across rather than generating a new
        // one, or every saved credential would become undecryptable.
        let key = match std::fs::read(&self.legacy) {
            Ok(bytes) if bytes.len() == KEY_LEN => {
                let mut key = [0u8; KEY_LEN];
                key.copy_from_slice(&bytes);
                tracing::info!("migrating the credential key into the Android Keystore");
                key
            }
            _ => {
                let mut key = [0u8; KEY_LEN];
                getrandom::fill(&mut key).map_err(|e| err(e.to_string()))?;
                key
            }
        };

        let blob = wrap_key(&key).map_err(err)?;
        std::fs::write(&self.wrapped, &blob).map_err(|e| err(e.to_string()))?;

        // Only now that the wrapped copy is safely on disk.
        if self.legacy.exists() {
            let _ = std::fs::remove_file(&self.legacy);
        }
        Ok(key)
    }
}

/// Runs `f` with a JNI environment attached to the current thread.
fn with_env<T>(f: impl FnOnce(&mut JNIEnv) -> Result<T, jni::errors::Error>) -> Result<T, String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|e| format!("no Java VM: {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("could not attach to the Java VM: {e}"))?;
    f(&mut env).map_err(|e| {
        // A pending Java exception would poison every later JNI call.
        let _ = env.exception_clear();
        format!("{e}")
    })
}

/// Fetches the wrapping key, creating it on first use.
fn wrapping_key<'a>(env: &mut JNIEnv<'a>) -> Result<JObject<'a>, jni::errors::Error> {
    let store = env.call_static_method(
        "java/security/KeyStore",
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/KeyStore;",
        &[(&env.new_string("AndroidKeyStore")?).into()],
    )?;
    let store = store.l()?;
    env.call_method(
        &store,
        "load",
        "(Ljava/io/InputStream;[C)V",
        &[(&JObject::null()).into(), (&JObject::null()).into()],
    )?;

    let alias = env.new_string(ALIAS)?;
    let exists = env
        .call_method(
            &store,
            "containsAlias",
            "(Ljava/lang/String;)Z",
            &[(&alias).into()],
        )?
        .z()?;

    if !exists {
        generate_wrapping_key(env)?;
    }

    let alias = env.new_string(ALIAS)?;
    env.call_method(
        &store,
        "getKey",
        "(Ljava/lang/String;[C)Ljava/security/Key;",
        &[(&alias).into(), (&JObject::null()).into()],
    )?
    .l()
}

/// Creates the wrapping key inside the platform keystore.
///
/// It is never exported: everything below asks the platform to encrypt on our
/// behalf rather than handing us key material.
fn generate_wrapping_key(env: &mut JNIEnv) -> Result<(), jni::errors::Error> {
    let generator = env
        .call_static_method(
            "javax/crypto/KeyGenerator",
            "getInstance",
            "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
            &[
                (&env.new_string("AES")?).into(),
                (&env.new_string("AndroidKeyStore")?).into(),
            ],
        )?
        .l()?;

    let alias = env.new_string(ALIAS)?;
    let builder = env.new_object(
        "android/security/keystore/KeyGenParameterSpec$Builder",
        "(Ljava/lang/String;I)V",
        &[(&alias).into(), JValue::Int(PURPOSES)],
    )?;

    let string_class = env.find_class("java/lang/String")?;
    let gcm = env.new_string("GCM")?;
    let modes = env.new_object_array(1, &string_class, &gcm)?;
    let builder = env
        .call_method(
            &builder,
            "setBlockModes",
            "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[(&modes).into()],
        )?
        .l()?;

    let no_padding = env.new_string("NoPadding")?;
    let paddings = env.new_object_array(1, &string_class, &no_padding)?;
    let builder = env
        .call_method(
            &builder,
            "setEncryptionPaddings",
            "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[(&paddings).into()],
        )?
        .l()?;

    let builder = env
        .call_method(
            &builder,
            "setKeySize",
            "(I)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[JValue::Int(256)],
        )?
        .l()?;

    let spec = env
        .call_method(
            &builder,
            "build",
            "()Landroid/security/keystore/KeyGenParameterSpec;",
            &[],
        )?
        .l()?;

    env.call_method(
        &generator,
        "init",
        "(Ljava/security/spec/AlgorithmParameterSpec;)V",
        &[(&spec).into()],
    )?;
    env.call_method(&generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[])?;
    Ok(())
}

/// Encrypts the database key, returning `IV || ciphertext`.
fn wrap_key(key: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    with_env(|env| {
        let wrapping = wrapping_key(env)?;
        let cipher = env
            .call_static_method(
                "javax/crypto/Cipher",
                "getInstance",
                "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
                &[(&env.new_string("AES/GCM/NoPadding")?).into()],
            )?
            .l()?;

        env.call_method(
            &cipher,
            "init",
            "(ILjava/security/Key;)V",
            &[JValue::Int(ENCRYPT_MODE), (&wrapping).into()],
        )?;

        let iv = env
            .call_method(&cipher, "getIV", "()[B", &[])?
            .l()
            .map(JByteArray::from)?;
        let iv = env.convert_byte_array(&iv)?;

        let input = env.byte_array_from_slice(key)?;
        let out = env
            .call_method(&cipher, "doFinal", "([B)[B", &[(&input).into()])?
            .l()
            .map(JByteArray::from)?;
        let mut out = env.convert_byte_array(&out)?;

        let mut blob = iv;
        blob.append(&mut out);
        Ok(blob)
    })
}

/// Reverses [`wrap_key`].
fn unwrap_key(blob: &[u8]) -> Result<[u8; KEY_LEN], String> {
    if blob.len() <= IV_LEN {
        return Err(format!("wrapped key is {} bytes, too short", blob.len()));
    }
    let (iv, ciphertext) = blob.split_at(IV_LEN);

    let plain = with_env(|env| {
        let wrapping = wrapping_key(env)?;
        let cipher = env
            .call_static_method(
                "javax/crypto/Cipher",
                "getInstance",
                "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
                &[(&env.new_string("AES/GCM/NoPadding")?).into()],
            )?
            .l()?;

        let iv_array = env.byte_array_from_slice(iv)?;
        let spec = env.new_object(
            "javax/crypto/spec/GCMParameterSpec",
            "(I[B)V",
            &[JValue::Int(TAG_BITS), (&iv_array).into()],
        )?;

        env.call_method(
            &cipher,
            "init",
            "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
            &[
                JValue::Int(DECRYPT_MODE),
                (&wrapping).into(),
                (&spec).into(),
            ],
        )?;

        let input = env.byte_array_from_slice(ciphertext)?;
        let out = env
            .call_method(&cipher, "doFinal", "([B)[B", &[(&input).into()])?
            .l()
            .map(JByteArray::from)?;
        env.convert_byte_array(&out)
    })?;

    if plain.len() != KEY_LEN {
        return Err(format!(
            "unwrapped key is {} bytes, expected {KEY_LEN}",
            plain.len()
        ));
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&plain);
    Ok(key)
}
