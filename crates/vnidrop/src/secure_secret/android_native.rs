use jni::{
    errors::Error as JniError,
    objects::{JByteArray, JObject, JString, JValue},
    JNIEnv, JavaVM,
};

use super::*;

const ANDROID_KEYSTORE: &str = "AndroidKeyStore";
const AES: &str = "AES";
const TRANSFORMATION: &str = "AES/GCM/NoPadding";

/// JNI-backed Android Keystore engine. The VM pointer comes from the Android
/// runtime; no Context or secret bytes are exposed through UniFFI.
pub(crate) struct AndroidJniKeystore {
    vm: JavaVM,
}

impl AndroidJniKeystore {
    /// Constructs the engine after the Android runtime has initialized
    /// `ndk-context` with the process Java VM.
    pub(crate) fn from_android_runtime() -> Result<Self, SecureSecretStoreError> {
        let context = std::panic::catch_unwind(ndk_context::android_context)
            .map_err(|_| SecureSecretStoreError::Unavailable)?;
        let vm = context.vm();
        if vm.is_null() {
            return Err(SecureSecretStoreError::Unavailable);
        }
        // Android owns the process VM for longer than every core instance.
        let vm = unsafe { JavaVM::from_raw(vm.cast()) }
            .map_err(|_| SecureSecretStoreError::Unavailable)?;
        Ok(Self { vm })
    }

    fn with_env<T>(
        &self,
        operation: impl FnOnce(&mut JNIEnv<'_>) -> Result<T, SecureSecretStoreError>,
    ) -> Result<T, SecureSecretStoreError> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|_| SecureSecretStoreError::Unavailable)?;
        operation(&mut env)
    }

    fn no_backup_files_dir(&self) -> Result<PathBuf, SecureSecretStoreError> {
        self.with_env(|env| {
            let context = std::panic::catch_unwind(ndk_context::android_context)
                .map_err(|_| SecureSecretStoreError::Unavailable)?
                .context();
            if context.is_null() {
                return Err(SecureSecretStoreError::Unavailable);
            }
            // ndk-context retains this process Context for the Android runtime lifetime.
            let context = unsafe { JObject::from_raw(context.cast()) };
            let directory = env
                .call_method(&context, "getNoBackupFilesDir", "()Ljava/io/File;", &[])
                .map_err(|error| map_jni_error(env, error))?
                .l()
                .map_err(|error| map_jni_error(env, error))?;
            if directory.is_null() {
                return Err(SecureSecretStoreError::Unavailable);
            }
            let path = env
                .call_method(&directory, "getAbsolutePath", "()Ljava/lang/String;", &[])
                .map_err(|error| map_jni_error(env, error))?
                .l()
                .map_err(|error| map_jni_error(env, error))?;
            let path = JString::from(path);
            let path: String = env
                .get_string(&path)
                .map_err(|error| map_jni_error(env, error))?
                .into();
            Ok(PathBuf::from(path))
        })
    }
}

pub(crate) fn create_store_from_android_runtime(
) -> Result<Arc<dyn SecureSecretStore>, SecureSecretStoreError> {
    let keystore = Arc::new(AndroidJniKeystore::from_android_runtime()?);
    let no_backup_dir = keystore.no_backup_files_dir()?;
    Ok(Arc::new(AndroidSecureSecretStore::new(
        &no_backup_dir,
        keystore,
    )?))
}

impl AndroidKeystore for AndroidJniKeystore {
    fn seal(
        &self,
        alias: &str,
        plaintext: &[u8],
    ) -> Result<AndroidSealedValue, SecureSecretStoreError> {
        self.with_env(|env| {
            let key_store = load_key_store(env)?;
            let alias_string = env
                .new_string(alias)
                .map_err(|error| map_jni_error(env, error))?;
            let alias_object = JObject::from(alias_string);
            let contains = env
                .call_method(
                    &key_store,
                    "containsAlias",
                    "(Ljava/lang/String;)Z",
                    &[JValue::Object(&alias_object)],
                )
                .map_err(|error| map_jni_error(env, error))?
                .z()
                .map_err(|error| map_jni_error(env, error))?;
            if !contains {
                generate_key(env, alias)?;
            }
            let key = get_key(env, &key_store, alias)?;
            let cipher = cipher_instance(env)?;
            env.call_method(
                &cipher,
                "init",
                "(ILjava/security/Key;)V",
                &[JValue::Int(1), JValue::Object(&key)],
            )
            .map_err(|error| map_jni_error(env, error))?;
            let plaintext = env
                .byte_array_from_slice(plaintext)
                .map_err(|error| map_jni_error(env, error))?;
            let plaintext_object = JObject::from(plaintext);
            let ciphertext = env
                .call_method(
                    &cipher,
                    "doFinal",
                    "([B)[B",
                    &[JValue::Object(&plaintext_object)],
                )
                .map_err(|error| map_jni_error(env, error))?
                .l()
                .map_err(|error| map_jni_error(env, error))?;
            let nonce = env
                .call_method(&cipher, "getIV", "()[B", &[])
                .map_err(|error| map_jni_error(env, error))?
                .l()
                .map_err(|error| map_jni_error(env, error))?;
            Ok(AndroidSealedValue {
                nonce: env
                    .convert_byte_array(JByteArray::from(nonce))
                    .map_err(|error| map_jni_error(env, error))?,
                ciphertext: env
                    .convert_byte_array(JByteArray::from(ciphertext))
                    .map_err(|error| map_jni_error(env, error))?,
            })
        })
    }

    fn open(
        &self,
        alias: &str,
        sealed: &AndroidSealedValue,
    ) -> Result<Vec<u8>, SecureSecretStoreError> {
        self.with_env(|env| {
            let key_store = load_key_store(env)?;
            let key = get_key(env, &key_store, alias)?;
            let cipher = cipher_instance(env)?;
            let nonce = env
                .byte_array_from_slice(&sealed.nonce)
                .map_err(|error| map_jni_error(env, error))?;
            let nonce_object = JObject::from(nonce);
            let parameters = env
                .new_object(
                    "javax/crypto/spec/GCMParameterSpec",
                    "(I[B)V",
                    &[JValue::Int(128), JValue::Object(&nonce_object)],
                )
                .map_err(|error| map_jni_error(env, error))?;
            env.call_method(
                &cipher,
                "init",
                "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
                &[
                    JValue::Int(2),
                    JValue::Object(&key),
                    JValue::Object(&parameters),
                ],
            )
            .map_err(|error| map_jni_error(env, error))?;
            let ciphertext = env
                .byte_array_from_slice(&sealed.ciphertext)
                .map_err(|error| map_jni_error(env, error))?;
            let ciphertext_object = JObject::from(ciphertext);
            let plaintext = env
                .call_method(
                    &cipher,
                    "doFinal",
                    "([B)[B",
                    &[JValue::Object(&ciphertext_object)],
                )
                .map_err(|error| map_jni_error(env, error))?
                .l()
                .map_err(|error| map_jni_error(env, error))?;
            env.convert_byte_array(JByteArray::from(plaintext))
                .map_err(|error| map_jni_error(env, error))
        })
    }

    fn delete(&self, alias: &str) -> Result<(), SecureSecretStoreError> {
        self.with_env(|env| {
            let key_store = load_key_store(env)?;
            let alias = env
                .new_string(alias)
                .map_err(|error| map_jni_error(env, error))?;
            let alias_object = JObject::from(alias);
            env.call_method(
                &key_store,
                "deleteEntry",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&alias_object)],
            )
            .map_err(|error| map_jni_error(env, error))?;
            Ok(())
        })
    }
}

fn load_key_store<'local>(
    env: &mut JNIEnv<'local>,
) -> Result<JObject<'local>, SecureSecretStoreError> {
    let provider = env
        .new_string(ANDROID_KEYSTORE)
        .map_err(|error| map_jni_error(env, error))?;
    let provider_object = JObject::from(provider);
    let key_store = env
        .call_static_method(
            "java/security/KeyStore",
            "getInstance",
            "(Ljava/lang/String;)Ljava/security/KeyStore;",
            &[JValue::Object(&provider_object)],
        )
        .map_err(|error| map_jni_error(env, error))?
        .l()
        .map_err(|error| map_jni_error(env, error))?;
    env.call_method(
        &key_store,
        "load",
        "(Ljava/security/KeyStore$LoadStoreParameter;)V",
        &[JValue::Object(&JObject::null())],
    )
    .map_err(|error| map_jni_error(env, error))?;
    Ok(key_store)
}

fn generate_key(env: &mut JNIEnv<'_>, alias: &str) -> Result<(), SecureSecretStoreError> {
    let algorithm = env
        .new_string(AES)
        .map_err(|error| map_jni_error(env, error))?;
    let provider = env
        .new_string(ANDROID_KEYSTORE)
        .map_err(|error| map_jni_error(env, error))?;
    let algorithm_object = JObject::from(algorithm);
    let provider_object = JObject::from(provider);
    let generator = env
        .call_static_method(
            "javax/crypto/KeyGenerator",
            "getInstance",
            "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
            &[
                JValue::Object(&algorithm_object),
                JValue::Object(&provider_object),
            ],
        )
        .map_err(|error| map_jni_error(env, error))?
        .l()
        .map_err(|error| map_jni_error(env, error))?;
    let alias = env
        .new_string(alias)
        .map_err(|error| map_jni_error(env, error))?;
    let alias_object = JObject::from(alias);
    let builder = env
        .new_object(
            "android/security/keystore/KeyGenParameterSpec$Builder",
            "(Ljava/lang/String;I)V",
            &[JValue::Object(&alias_object), JValue::Int(3)],
        )
        .map_err(|error| map_jni_error(env, error))?;
    let modes = java_string_array(env, "GCM")?;
    env.call_method(
        &builder,
        "setBlockModes",
        "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        &[JValue::Object(&modes)],
    )
    .map_err(|error| map_jni_error(env, error))?;
    let paddings = java_string_array(env, "NoPadding")?;
    env.call_method(
        &builder,
        "setEncryptionPaddings",
        "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        &[JValue::Object(&paddings)],
    )
    .map_err(|error| map_jni_error(env, error))?;
    env.call_method(
        &builder,
        "setKeySize",
        "(I)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        &[JValue::Int(256)],
    )
    .map_err(|error| map_jni_error(env, error))?;
    env.call_method(
        &builder,
        "setRandomizedEncryptionRequired",
        "(Z)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        &[JValue::Bool(1)],
    )
    .map_err(|error| map_jni_error(env, error))?;
    let parameters = env
        .call_method(
            &builder,
            "build",
            "()Landroid/security/keystore/KeyGenParameterSpec;",
            &[],
        )
        .map_err(|error| map_jni_error(env, error))?
        .l()
        .map_err(|error| map_jni_error(env, error))?;
    env.call_method(
        &generator,
        "init",
        "(Ljava/security/spec/AlgorithmParameterSpec;)V",
        &[JValue::Object(&parameters)],
    )
    .map_err(|error| map_jni_error(env, error))?;
    env.call_method(&generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[])
        .map_err(|error| map_jni_error(env, error))?;
    Ok(())
}

fn get_key<'local>(
    env: &mut JNIEnv<'local>,
    key_store: &JObject<'local>,
    alias: &str,
) -> Result<JObject<'local>, SecureSecretStoreError> {
    let alias = env
        .new_string(alias)
        .map_err(|error| map_jni_error(env, error))?;
    let alias_object = JObject::from(alias);
    let key = env
        .call_method(
            key_store,
            "getKey",
            "(Ljava/lang/String;[C)Ljava/security/Key;",
            &[
                JValue::Object(&alias_object),
                JValue::Object(&JObject::null()),
            ],
        )
        .map_err(|error| map_jni_error(env, error))?
        .l()
        .map_err(|error| map_jni_error(env, error))?;
    if key.is_null() {
        return Err(SecureSecretStoreError::Missing);
    }
    Ok(key)
}

fn cipher_instance<'local>(
    env: &mut JNIEnv<'local>,
) -> Result<JObject<'local>, SecureSecretStoreError> {
    let transformation = env
        .new_string(TRANSFORMATION)
        .map_err(|error| map_jni_error(env, error))?;
    let transformation_object = JObject::from(transformation);
    env.call_static_method(
        "javax/crypto/Cipher",
        "getInstance",
        "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
        &[JValue::Object(&transformation_object)],
    )
    .map_err(|error| map_jni_error(env, error))?
    .l()
    .map_err(|error| map_jni_error(env, error))
}

fn java_string_array<'local>(
    env: &mut JNIEnv<'local>,
    value: &str,
) -> Result<JObject<'local>, SecureSecretStoreError> {
    let class = env
        .find_class("java/lang/String")
        .map_err(|error| map_jni_error(env, error))?;
    let array = env
        .new_object_array(1, class, JObject::null())
        .map_err(|error| map_jni_error(env, error))?;
    let value = env
        .new_string(value)
        .map_err(|error| map_jni_error(env, error))?;
    env.set_object_array_element(&array, 0, value)
        .map_err(|error| map_jni_error(env, error))?;
    Ok(JObject::from(array))
}

fn map_jni_error(env: &mut JNIEnv<'_>, _error: JniError) -> SecureSecretStoreError {
    let has_exception = env.exception_check().unwrap_or(false);
    if !has_exception {
        return SecureSecretStoreError::Unavailable;
    }
    let exception = match env.exception_occurred() {
        Ok(exception) => exception,
        Err(_) => return SecureSecretStoreError::Unavailable,
    };
    let _ = env.exception_clear();
    if is_instance_of(
        env,
        &exception,
        "android/security/keystore/UserNotAuthenticatedException",
    ) {
        SecureSecretStoreError::Locked
    } else if is_instance_of(
        env,
        &exception,
        "android/security/keystore/KeyPermanentlyInvalidatedException",
    ) || is_instance_of(env, &exception, "javax/crypto/AEADBadTagException")
        || is_instance_of(env, &exception, "javax/crypto/BadPaddingException")
    {
        SecureSecretStoreError::Corrupted
    } else if is_instance_of(env, &exception, "java/security/UnrecoverableKeyException") {
        SecureSecretStoreError::Missing
    } else {
        SecureSecretStoreError::Unavailable
    }
}

fn is_instance_of(env: &mut JNIEnv<'_>, object: &JObject<'_>, class: &str) -> bool {
    env.is_instance_of(object, class).unwrap_or(false)
}
