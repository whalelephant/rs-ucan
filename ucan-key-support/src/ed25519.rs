use anyhow::{anyhow, Result};
use async_trait::async_trait;

use ed25519_zebra::{
    Signature, SigningKey as Ed25519PrivateKey, VerificationKey as Ed25519PublicKey,
};

use ucan::crypto::KeyMaterial;

pub const ED25519_MAGIC_BYTES: [u8; 2] = [0xed, 0x01];

pub fn bytes_to_ed25519_key(bytes: Vec<u8>) -> Result<Box<dyn KeyMaterial>> {
    let public_key = Ed25519PublicKey::try_from(bytes.as_slice())?;
    Ok(Box::new(Ed25519KeyMaterial(public_key, None)))
}

#[derive(Clone)]
pub struct Ed25519KeyMaterial(pub Ed25519PublicKey, pub Option<Ed25519PrivateKey>);

#[cfg_attr(all(target_arch="wasm32", feature = "web"), async_trait(?Send))]
#[cfg_attr(any(not(target_arch = "wasm32"), not(feature = "web")), async_trait)]
impl KeyMaterial for Ed25519KeyMaterial {
    fn get_jwt_algorithm_name(&self) -> String {
        "EdDSA".into()
    }

    async fn get_did(&self) -> Result<String> {
        let bytes = [ED25519_MAGIC_BYTES.as_slice(), self.0.as_ref()].concat();
        Ok(format!("did:key:z{}", bs58::encode(bytes).into_string()))
    }

    async fn sign(&self, payload: &[u8]) -> Result<Vec<u8>> {
        match self.1 {
            Some(private_key) => {
                let signature = private_key.sign(payload);
                let bytes: [u8; 64] = signature.into();
                Ok(bytes.to_vec())
            }
            None => Err(anyhow!("No private key; cannot sign data")),
        }
    }

    async fn verify(&self, payload: &[u8], signature: &[u8]) -> Result<()> {
        let signature = Signature::try_from(signature)?;
        self.0
            .verify(&signature, payload)
            .map_err(|error| anyhow!(error))
    }
}

#[cfg(test)]
mod tests {
    use super::{bytes_to_ed25519_key, Ed25519KeyMaterial, ED25519_MAGIC_BYTES};
    use ed25519_zebra::{SigningKey as Ed25519PrivateKey, VerificationKey as Ed25519PublicKey};
    use ucan::{
        builder::UcanBuilder,
        crypto::{did::DidParser, KeyMaterial},
        ucan::Ucan,
    };

    #[tokio::test]
    async fn it_can_sign_and_verify_a_ucan() {
        let rng = rand::thread_rng();
        let private_key = Ed25519PrivateKey::new(rng);
        let public_key = Ed25519PublicKey::from(&private_key);

        let key_material = Ed25519KeyMaterial(public_key, Some(private_key));
        let token_string = UcanBuilder::new()
            .issued_by(&key_material)
            .for_audience(key_material.get_did().await.unwrap().as_str())
            .with_lifetime(60)
            .build()
            .unwrap()
            .sign()
            .await
            .unwrap()
            .encode()
            .unwrap();

        let did_parser = DidParser::new(&[(ED25519_MAGIC_BYTES, bytes_to_ed25519_key)]);

        let ucan = Ucan::try_from_token_string(token_string.as_str()).unwrap();
        ucan.check_signature(did_parser.clone()).await.unwrap();
    }
}
