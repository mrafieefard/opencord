//! A certificate authority for tests: certificates an authority signed, for
//! names like `chat.example.test`, without the internet. Clients told to
//! trust [`TestCa::ca_pem`] check them as they would Let's Encrypt's.

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};

pub struct TestCa {
    issuer: CertifiedIssuer<'static, KeyPair>,
}

/// A certificate and its key, in PEM.
pub struct Issued {
    pub cert_pem: String,
    pub key_pem: String,
}

impl TestCa {
    pub fn new() -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("no names is valid");
        params
            .distinguished_name
            .push(DnType::CommonName, "Opencord test CA");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let key = KeyPair::generate().expect("a key can be made");
        Self {
            issuer: CertifiedIssuer::self_signed(params, key).expect("the CA signs itself"),
        }
    }

    pub fn ca_pem(&self) -> String {
        self.issuer.pem()
    }

    pub fn ca_der(&self) -> Vec<u8> {
        self.issuer.der().to_vec()
    }

    /// A certificate for `names`, signed by this authority.
    pub fn issue(&self, names: &[&str]) -> Issued {
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        let params = CertificateParams::new(names).expect("valid names");
        let key = KeyPair::generate().expect("a key can be made");
        let issuer: &Issuer<'_, KeyPair> = &self.issuer;
        let cert = params.signed_by(&key, issuer).expect("the CA signs it");
        Issued {
            cert_pem: cert.pem(),
            key_pem: key.serialize_pem(),
        }
    }
}

impl Default for TestCa {
    fn default() -> Self {
        Self::new()
    }
}
