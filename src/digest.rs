use std::path::PathBuf;

use clap::error::{ContextKind, ContextValue, ErrorKind};

pub fn parse_signing_key(s: &str) -> Result<ed25519_dalek::SigningKey, clap::Error> {
    let mut err = clap::Error::new(ErrorKind::ValueValidation);
    err.insert(
        ContextKind::InvalidValue,
        ContextValue::String(s.to_owned()),
    );

    let val = PathBuf::from(s);
    match val.as_path().try_exists() {
        Ok(true) => {
            let key_bytes = std::fs::read(val)?;
            match ssh_key::PrivateKey::from_openssh(key_bytes) {
                Ok(key) => {
                    if key.is_encrypted() {
                        err.insert(
                            ContextKind::Suggested,
                            ContextValue::String("encrypted keys are not supported".to_owned()),
                        );
                    } else if let Some(key) = key.key_data().ed25519() {
                        return Ok(ed25519_dalek::SigningKey::from_bytes(
                            &key.private.to_bytes(),
                        ));
                    } else {
                        err.insert(
                            ContextKind::Suggested,
                            ContextValue::String(
                                "only ed25519 keys are supported currently".to_owned(),
                            ),
                        );
                    }
                }
                Err(e) => {
                    err.insert(ContextKind::Suggested, ContextValue::String(e.to_string()));
                }
            }
        }
        Ok(false) => {
            err.insert(
                ContextKind::Suggested,
                ContextValue::String("file not found".to_owned()),
            );
        }
        Err(e) => {
            err.insert(ContextKind::Suggested, ContextValue::String(e.to_string()));
        }
    };
    Err(err)
}

pub fn parse_verifying_key(s: &str) -> Result<ed25519_dalek::VerifyingKey, clap::Error> {
    let mut err = clap::Error::new(ErrorKind::ValueValidation);
    err.insert(
        ContextKind::InvalidValue,
        ContextValue::String(s.to_owned()),
    );

    match ssh_key::PublicKey::from_openssh(s) {
        Ok(key) => {
            if let Some(key) = key.key_data().ed25519() {
                match ed25519_dalek::VerifyingKey::from_bytes(&key.0) {
                    Ok(key) => return Ok(key),
                    Err(e) => {
                        err.insert(ContextKind::Suggested, ContextValue::String(e.to_string()));
                    }
                }
            } else {
                err.insert(
                    ContextKind::Suggested,
                    ContextValue::String("only ed25519 keys are supported currently".to_owned()),
                );
            }
        }
        Err(e) => {
            err.insert(ContextKind::Suggested, ContextValue::String(e.to_string()));
        }
    }

    Err(err)
}
