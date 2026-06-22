const KEYCHAIN_PREFIX: &str = "keychain:";

pub fn store_keychain_secret(reference: &str, secret: &str) -> Result<(), String> {
    let target = parse_keychain_reference(reference)?;
    platform::store(&target, secret)
}

pub fn load_keychain_secret(reference: &str) -> Result<String, String> {
    let target = parse_keychain_reference(reference)?;
    platform::load(&target)
}

fn parse_keychain_reference(reference: &str) -> Result<String, String> {
    let value = reference
        .strip_prefix(KEYCHAIN_PREFIX)
        .ok_or_else(|| "Credential reference must start with keychain:".to_string())?;
    let mut segments = value.split('/');
    let service = segments
        .next()
        .filter(|part| !part.trim().is_empty())
        .ok_or_else(|| "Credential reference is missing a keychain service.".to_string())?;
    let account = segments.collect::<Vec<_>>().join("/");
    if account.trim().is_empty() {
        return Err("Credential reference is missing a keychain account.".to_string());
    }

    Ok(format!("{service}/{account}"))
}

#[cfg(any(target_os = "macos", test))]
fn split_keychain_target(target: &str) -> Result<(&str, &str), String> {
    let (service, account) = target
        .split_once('/')
        .ok_or_else(|| "Credential target is missing a keychain account.".to_string())?;
    if service.trim().is_empty() {
        return Err("Credential target is missing a keychain service.".to_string());
    }
    if account.trim().is_empty() {
        return Err("Credential target is missing a keychain account.".to_string());
    }
    Ok((service, account))
}

#[cfg(windows)]
mod platform {
    use std::mem::size_of;
    use std::ptr::null_mut;
    use std::slice;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    pub fn store(target: &str, secret: &str) -> Result<(), String> {
        let mut target_wide = to_wide_null(target);
        let mut username_wide = to_wide_null("CodeStudio Lite");
        let mut secret_wide = to_wide(secret);
        let blob_size = secret_wide
            .len()
            .checked_mul(size_of::<u16>())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| {
                "Provider API key is too large for Windows Credential Manager.".to_string()
            })?;

        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target_wide.as_mut_ptr(),
            Comment: null_mut(),
            LastWritten: Default::default(),
            CredentialBlobSize: blob_size,
            CredentialBlob: secret_wide.as_mut_ptr().cast::<u8>(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: null_mut(),
            TargetAlias: null_mut(),
            UserName: username_wide.as_mut_ptr(),
        };

        // Windows copies the credential data during CredWriteW; the stack-owned buffers
        // only need to stay alive for this call.
        let ok = unsafe { CredWriteW(&credential, 0) };
        if ok == 0 {
            return Err(format!(
                "Could not store Provider API key in Windows Credential Manager: {}",
                last_error()
            ));
        }

        Ok(())
    }

    pub fn load(target: &str) -> Result<String, String> {
        let target_wide = to_wide_null(target);
        let mut credential_ptr: *mut CREDENTIALW = null_mut();
        let ok = unsafe {
            CredReadW(
                target_wide.as_ptr(),
                CRED_TYPE_GENERIC,
                0,
                &mut credential_ptr,
            )
        };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            if code == ERROR_NOT_FOUND {
                return Err(
                    "Provider API key is not stored in the system keychain yet.".to_string()
                );
            }

            return Err(format!(
                "Could not read Provider API key from Windows Credential Manager: error {code}"
            ));
        }
        if credential_ptr.is_null() {
            return Err("Windows Credential Manager returned an empty credential.".to_string());
        }

        let result = unsafe {
            let credential = &*credential_ptr;
            let byte_len = usize::try_from(credential.CredentialBlobSize)
                .map_err(|_| "Stored Provider API key is too large.".to_string())?;
            if byte_len % size_of::<u16>() != 0 {
                return Err("Stored Provider API key is not valid UTF-16 data.".to_string());
            }

            let units = slice::from_raw_parts(
                credential.CredentialBlob.cast::<u16>(),
                byte_len / size_of::<u16>(),
            );
            String::from_utf16(units)
                .map_err(|_| "Stored Provider API key is not valid UTF-16 data.".to_string())
        };

        unsafe {
            CredFree(credential_ptr.cast());
        }

        result
    }

    fn to_wide(value: &str) -> Vec<u16> {
        value.encode_utf16().collect()
    }

    fn to_wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain([0]).collect()
    }

    fn last_error() -> String {
        format!("error {}", unsafe { GetLastError() })
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::split_keychain_target;
    use std::ffi::{c_char, c_void};
    use std::ptr::{null, null_mut};

    const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    type SecKeychainItemRef = *const c_void;

    #[link(name = "Security", kind = "framework")]
    extern "C" {
        fn SecKeychainAddGenericPassword(
            keychain: *const c_void,
            service_name_length: u32,
            service_name: *const c_char,
            account_name_length: u32,
            account_name: *const c_char,
            password_length: u32,
            password_data: *const c_void,
            item_ref: *mut SecKeychainItemRef,
        ) -> i32;

        fn SecKeychainFindGenericPassword(
            keychain: *const c_void,
            service_name_length: u32,
            service_name: *const c_char,
            account_name_length: u32,
            account_name: *const c_char,
            password_length: *mut u32,
            password_data: *mut *mut c_void,
            item_ref: *mut SecKeychainItemRef,
        ) -> i32;

        fn SecKeychainItemModifyAttributesAndData(
            item_ref: SecKeychainItemRef,
            attr_list: *const c_void,
            length: u32,
            data: *const c_void,
        ) -> i32;

        fn SecKeychainItemFreeContent(attr_list: *const c_void, data: *mut c_void) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    pub fn store(target: &str, secret: &str) -> Result<(), String> {
        let (service, account) = split_keychain_target(target)?;
        let service_len = keychain_len("service", service)?;
        let account_len = keychain_len("account", account)?;
        let password = secret.as_bytes();
        let password_len = u32::try_from(password.len())
            .map_err(|_| "Provider API key is too large for macOS Keychain.".to_string())?;

        let mut item_ref: SecKeychainItemRef = null();
        let status = unsafe {
            SecKeychainAddGenericPassword(
                null(),
                service_len,
                service.as_ptr().cast::<c_char>(),
                account_len,
                account.as_ptr().cast::<c_char>(),
                password_len,
                password.as_ptr().cast::<c_void>(),
                &mut item_ref,
            )
        };
        if !item_ref.is_null() {
            unsafe { CFRelease(item_ref) };
        }

        if status == 0 {
            return Ok(());
        }
        if status == ERR_SEC_DUPLICATE_ITEM {
            return update_existing(service, account, password);
        }

        Err(format!(
            "Could not store Provider API key in macOS Keychain: {}",
            status_message(status)
        ))
    }

    pub fn load(target: &str) -> Result<String, String> {
        let (service, account) = split_keychain_target(target)?;
        let service_len = keychain_len("service", service)?;
        let account_len = keychain_len("account", account)?;
        let mut password_len = 0_u32;
        let mut password_data: *mut c_void = null_mut();

        let status = unsafe {
            SecKeychainFindGenericPassword(
                null(),
                service_len,
                service.as_ptr().cast::<c_char>(),
                account_len,
                account.as_ptr().cast::<c_char>(),
                &mut password_len,
                &mut password_data,
                null_mut(),
            )
        };
        if status == ERR_SEC_ITEM_NOT_FOUND {
            return Err("Provider API key is not stored in the system keychain yet.".to_string());
        }
        if status != 0 {
            return Err(format!(
                "Could not read Provider API key from macOS Keychain: {}",
                status_message(status)
            ));
        }
        if password_data.is_null() {
            return Err("macOS Keychain returned an empty credential.".to_string());
        }

        let result = unsafe {
            let bytes = std::slice::from_raw_parts(
                password_data.cast::<u8>(),
                usize::try_from(password_len)
                    .map_err(|_| "Stored Provider API key is too large.".to_string())?,
            );
            String::from_utf8(bytes.to_vec())
                .map_err(|_| "Stored Provider API key is not valid UTF-8 data.".to_string())
        };
        unsafe {
            SecKeychainItemFreeContent(null(), password_data);
        }
        result
    }

    fn update_existing(service: &str, account: &str, password: &[u8]) -> Result<(), String> {
        let service_len = keychain_len("service", service)?;
        let account_len = keychain_len("account", account)?;
        let password_len = u32::try_from(password.len())
            .map_err(|_| "Provider API key is too large for macOS Keychain.".to_string())?;
        let mut item_ref: SecKeychainItemRef = null();
        let status = unsafe {
            SecKeychainFindGenericPassword(
                null(),
                service_len,
                service.as_ptr().cast::<c_char>(),
                account_len,
                account.as_ptr().cast::<c_char>(),
                null_mut(),
                null_mut(),
                &mut item_ref,
            )
        };
        if status != 0 {
            return Err(format!(
                "Could not find existing Provider API key in macOS Keychain: {}",
                status_message(status)
            ));
        }
        if item_ref.is_null() {
            return Err("macOS Keychain returned an empty item reference.".to_string());
        }

        let update_status = unsafe {
            SecKeychainItemModifyAttributesAndData(
                item_ref,
                null(),
                password_len,
                password.as_ptr().cast::<c_void>(),
            )
        };
        unsafe { CFRelease(item_ref) };
        if update_status != 0 {
            return Err(format!(
                "Could not update Provider API key in macOS Keychain: {}",
                status_message(update_status)
            ));
        }

        Ok(())
    }

    fn keychain_len(label: &str, value: &str) -> Result<u32, String> {
        if value.as_bytes().contains(&0) {
            return Err(format!("Credential {label} must not contain NUL bytes."));
        }
        u32::try_from(value.len())
            .map_err(|_| format!("Credential {label} is too large for macOS Keychain."))
    }

    fn status_message(status: i32) -> String {
        match status {
            ERR_SEC_DUPLICATE_ITEM => "item already exists".to_string(),
            ERR_SEC_ITEM_NOT_FOUND => "item not found".to_string(),
            _ => format!("OSStatus {status}"),
        }
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
mod platform {
    pub fn store(_target: &str, _secret: &str) -> Result<(), String> {
        Err("System keychain is not implemented on this platform yet.".to_string())
    }

    pub fn load(_target: &str) -> Result<String, String> {
        Err("System keychain is not implemented on this platform yet.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_reference_preserves_account_slashes() {
        assert_eq!(
            parse_keychain_reference("keychain:codestudio-lite/profile/api_key").as_deref(),
            Ok("codestudio-lite/profile/api_key")
        );
        assert_eq!(
            split_keychain_target("codestudio-lite/profile/api_key"),
            Ok(("codestudio-lite", "profile/api_key"))
        );
    }

    #[test]
    fn keychain_reference_rejects_missing_parts() {
        assert!(parse_keychain_reference("codestudio-lite/profile").is_err());
        assert!(parse_keychain_reference("keychain:/profile").is_err());
        assert!(parse_keychain_reference("keychain:codestudio-lite/").is_err());
        assert!(split_keychain_target("codestudio-lite").is_err());
        assert!(split_keychain_target("/profile").is_err());
        assert!(split_keychain_target("codestudio-lite/").is_err());
    }
}
