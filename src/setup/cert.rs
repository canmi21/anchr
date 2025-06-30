/* src/setup/cert.rs */

use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::x509::extension::{BasicConstraints, SubjectAlternativeName};
use openssl::x509::{X509Builder, X509NameBuilder};
use std::fs::File;
use std::io::Write;

pub fn generate_certificate(cert_path: &str, key_path: &str, ip_address: &str) {
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
    builder.set_version(2).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    builder
        .set_not_before(&Asn1Time::days_from_now(0).unwrap())
        .unwrap();
    builder
        .set_not_after(&Asn1Time::days_from_now(365).unwrap())
        .unwrap();

    let mut serial = BigNum::new().unwrap();
    serial.rand(159, MsbOption::MAYBE_ZERO, false).unwrap();
    builder
        .set_serial_number(&serial.to_asn1_integer().unwrap())
        .unwrap();

    let basic_constraints = BasicConstraints::new().critical().build().unwrap();
    builder.append_extension(basic_constraints).unwrap();

    // Subject Alternative Name
    let subject_alternative_name = SubjectAlternativeName::new()
        .dns("localhost")
        .dns("*.localhost")
        .ip("127.0.0.1")
        .ip("::1")
        .ip(ip_address) // Add the dynamically selected IP here.
        .build(&builder.x509v3_context(None, None))
        .unwrap();
    builder.append_extension(subject_alternative_name).unwrap();

    builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    let cert = builder.build();

    let mut cert_file = File::create(cert_path).unwrap();
    cert_file.write_all(&cert.to_pem().unwrap()).unwrap();

    let mut key_file = File::create(key_path).unwrap();
    key_file
        .write_all(&pkey.private_key_to_pem_pkcs8().unwrap())
        .unwrap();
}
