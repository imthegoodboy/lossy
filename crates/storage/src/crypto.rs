use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use zeroize::Zeroizing;

use crate::{Result, StoreError};

pub struct Cipher(Aes256Gcm);

impl Cipher {
    pub fn new(key: &[u8]) -> Result<Self> {
        Aes256Gcm::new_from_slice(key)
            .map(Self)
            .map_err(|_| StoreError::KeyUnavailable)
    }

    pub fn seal(&self, plain: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce).map_err(|_| StoreError::Randomness)?;
        let encrypted = self
            .0
            .encrypt(&Nonce::from(nonce), Payload { msg: plain, aad })
            .map_err(|_| StoreError::Authentication)?;
        let mut output = nonce.to_vec();
        output.extend(encrypted);
        Ok(output)
    }

    pub fn open(&self, sealed: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        if sealed.len() < 28 {
            return Err(StoreError::Authentication);
        }
        let nonce: [u8; 12] = sealed[..12]
            .try_into()
            .map_err(|_| StoreError::Authentication)?;
        self.0
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &sealed[12..],
                    aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| StoreError::Authentication)
    }
}
