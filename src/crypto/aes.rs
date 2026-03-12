//! AES-128 CBC, PKCS7; create_encryptor_with_key, create_decryptor_with_key (re-export BlockEncryptMut, Pkcs7).

use aes::Aes128;
use cbc::cipher::KeyIvInit;

// Re-export traits và types để các module khác có thể sử dụng
pub use cbc::cipher::block_padding::Pkcs7;
pub use cbc::cipher::{BlockDecryptMut, BlockEncryptMut};

pub type Aes128CbcDec = cbc::Decryptor<Aes128>;
pub type Aes128CbcEnc = cbc::Encryptor<Aes128>;

/// Create encryptor with given key
pub fn create_encryptor_with_key(key: &str) -> Aes128CbcEnc {
    let iv = [0u8; 16];
    Aes128CbcEnc::new(key.as_bytes().into(), &iv.into())
}

/// Create decryptor with given key
pub fn create_decryptor_with_key(key: &str) -> Aes128CbcDec {
    let iv = [0u8; 16];
    Aes128CbcDec::new(key.as_bytes().into(), &iv.into())
}
