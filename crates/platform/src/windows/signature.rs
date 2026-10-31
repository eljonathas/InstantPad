//! The driver setup runs elevated, so a tampered download must never reach it.

use std::ffi::c_void;
use std::path::Path;

use instantpad_application::message::{Failure, TextKey};
use windows::Win32::Foundation::HWND;
use windows::Win32::Security::Cryptography::{
    CERT_CONTEXT, CERT_FIND_SUBJECT_CERT, CERT_INFO, CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
    CERT_QUERY_FORMAT_FLAG_BINARY, CERT_QUERY_OBJECT_FILE, CERT_X500_NAME_STR, CMSG_SIGNER_INFO,
    CMSG_SIGNER_INFO_PARAM, CertCloseStore, CertFindCertificateInStore, CertFreeCertificateContext,
    CertNameToStrW, CryptMsgClose, CryptMsgGetParam, CryptQueryObject, HCERTSTORE,
    PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_UI_NONE, WinVerifyTrust,
};
use windows::core::{HSTRING, PCWSTR};

use super::vb_audio::is_vendor_subject;

/// Refuses on any doubt: an unsigned file, a signature Windows does not trust, a signer that
/// cannot be read, or a signer other than the vendor.
pub fn verify(path: &Path) -> Result<(), Failure> {
    if !has_trusted_signature(path) {
        return Err(Failure::text(TextKey::DriverSignatureInvalid));
    }
    let signer =
        read_signer(path).ok_or_else(|| Failure::text(TextKey::DriverSignatureUnreadable))?;
    if is_vendor_subject(&signer) {
        Ok(())
    } else {
        Err(Failure::because(TextKey::DriverSignerUnexpected, signer))
    }
}

fn has_trusted_signature(path: &Path) -> bool {
    let wide = HSTRING::from(path);
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;

    // SAFETY: `data` points at `file`, which points at `wide`; all three outlive the call.
    // The state action is the default (ignore), so there is no state to close afterwards.
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    status == 0
}

/// The subject of the certificate that signed the file, like `CN=..., O=...`.
fn read_signer(path: &Path) -> Option<String> {
    let wide = HSTRING::from(path);
    let mut store = HCERTSTORE::default();
    let mut message: *mut c_void = std::ptr::null_mut();

    // SAFETY: `wide` is null-terminated. On success the store and message handles are ours,
    // and `Handles` releases them.
    unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide.as_ptr().cast(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            None,
            None,
            None,
            Some(&mut store),
            Some(&mut message),
            None,
        )
        .ok()?;
    }
    let handles = Handles { store, message };

    // SAFETY: the handles are valid until `handles` drops.
    unsafe {
        let mut size = 0;
        CryptMsgGetParam(message, CMSG_SIGNER_INFO_PARAM, 0, None, &mut size).ok()?;
        // u64 elements, so the buffer is aligned for the pointers inside the signer info.
        let mut buffer = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
        CryptMsgGetParam(
            message,
            CMSG_SIGNER_INFO_PARAM,
            0,
            Some(buffer.as_mut_ptr().cast()),
            &mut size,
        )
        .ok()?;
        let signer = &*buffer.as_ptr().cast::<CMSG_SIGNER_INFO>();

        let wanted = CERT_INFO {
            Issuer: signer.Issuer,
            SerialNumber: signer.SerialNumber,
            ..Default::default()
        };
        let certificate = CertFindCertificateInStore(
            handles.store,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_SUBJECT_CERT,
            Some((&wanted as *const CERT_INFO).cast()),
            None,
        );
        if certificate.is_null() {
            return None;
        }
        let subject = subject_of(&*certificate);
        let _ = CertFreeCertificateContext(Some(certificate));
        subject
    }
}

/// # Safety
/// `certificate` must be a valid context.
unsafe fn subject_of(certificate: &CERT_CONTEXT) -> Option<String> {
    // SAFETY: a certificate context always points at its info.
    let subject = unsafe { &(*certificate.pCertInfo).Subject };
    // SAFETY: with no buffer, it returns the length needed, including the final null.
    let length = unsafe { CertNameToStrW(X509_ASN_ENCODING, subject, CERT_X500_NAME_STR, None) };
    if length <= 1 {
        return None;
    }
    let mut text = vec![0u16; length as usize];
    // SAFETY: `text` has the length just asked for.
    unsafe {
        CertNameToStrW(
            X509_ASN_ENCODING,
            subject,
            CERT_X500_NAME_STR,
            Some(&mut text),
        )
    };
    Some(String::from_utf16_lossy(&text[..length as usize - 1]))
}

struct Handles {
    store: HCERTSTORE,
    message: *mut c_void,
}

impl Drop for Handles {
    fn drop(&mut self) {
        // SAFETY: both handles came from a successful `CryptQueryObject` and are closed once.
        unsafe {
            let _ = CryptMsgClose(Some(self.message));
            let _ = CertCloseStore(Some(self.store), 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn an_unsigned_file_is_refused() {
        let path =
            std::env::temp_dir().join(format!("instantpad-unsigned-{}.exe", std::process::id()));
        fs::write(&path, [0x4D, 0x5A, 0, 0]).unwrap();

        let result = verify(&path);
        let _ = fs::remove_file(&path);

        assert_eq!(result, Err(Failure::text(TextKey::DriverSignatureInvalid)));
    }
}
