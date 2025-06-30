/* src/setup/cert.rs */

use openssl::rsa::Rsa;
use openssl::pkey::PKey;
use openssl::x509::X509NameBuilder;
use openssl::x509::X509Builder;
use std::fs::File;
use std::io::Write;

pub fn generate_certificate(cert_path: &str, key_path: &str) {
    let rsa = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(rsa).unwrap();
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("C", "CN").unwrap();
    name.append_entry_by_text("ST", "GD").unwrap();
    name.append_entry_by_text("L", "SZ").unwrap();
    name.append_entry_by_text("O", "Acme, Inc.").unwrap();
    name.append_entry_by_text("CN", "localhost").unwrap();
    let name = name.build();
    let mut builder = X509Builder::new().unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    let not_before = openssl::asn1::Asn1Time::days_from_now(0).unwrap();
    let not_after = openssl::asn1::Asn1Time::days_from_now(365).unwrap();
    builder.set_not_before(&not_before).unwrap();
    builder.set_not_after(&not_after).unwrap();
    builder.sign(&pkey, openssl::hash::MessageDigest::sha256()).unwrap();
    let cert = builder.build();
    let mut cert_file = File::create(cert_path).unwrap();
    cert_file.write_all(&cert.to_pem().unwrap()).unwrap();
    let mut key_file = File::create(key_path).unwrap();
    key_file.write_all(&pkey.private_key_to_pem_pkcs8().unwrap()).unwrap();
}
