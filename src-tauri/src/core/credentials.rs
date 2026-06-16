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

#[cfg(not(windows))]
mod platform {
    pub fn store(_target: &str, _secret: &str) -> Result<(), String> {
        Err("System keychain is not implemented on this platform yet.".to_string())
    }

    pub fn load(_target: &str) -> Result<String, String> {
        Err("System keychain is not implemented on this platform yet.".to_string())
    }
}
