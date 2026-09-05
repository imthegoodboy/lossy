//! Current-user Windows key protection. No UI or machine-wide protection is enabled.
#![allow(unsafe_code)]

use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    },
    core::PCWSTR,
};
use zeroize::Zeroize;

use crate::{Result, StoreError};

pub fn protect(input: &[u8]) -> Result<Vec<u8>> {
    transform(input, true)
}

pub fn unprotect(input: &[u8]) -> Result<Vec<u8>> {
    transform(input, false)
}

fn transform(input: &[u8], protect: bool) -> Result<Vec<u8>> {
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len()).map_err(|_| StoreError::KeyUnavailable)?,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input lives for the call; DPAPI does not modify its input blob. Output is a
    // Windows-owned allocation, read only after success, wiped, and released with LocalFree.
    unsafe {
        let result = if protect {
            CryptProtectData(
                &input_blob,
                PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|_| StoreError::KeyUnavailable)?;
        if output.pbData.is_null() {
            return Err(StoreError::KeyUnavailable);
        }
        let bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let owned = bytes.to_vec();
        bytes.zeroize();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(owned)
    }
}
