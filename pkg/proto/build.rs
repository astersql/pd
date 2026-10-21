// Copyright 2026 AsterSQL.

use std::{env, fs, path::PathBuf, process::Command};

use sha2::{Digest, Sha256};

const VERSION: &str = "v0.0.0-20261016101550-f8ad079c8e98";
const ARCHIVE_SHA256: &str = "aff09d074d493874bfd7e3e63c766a8d93f42901ff235dde2d2ced0c7f2a615e";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=GOPROXY");
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = Command::new("go")
        .args([
            "mod",
            "download",
            "-json",
            &format!("github.com/pingcap/kvproto@{VERSION}"),
        ])
        .current_dir(manifest.join("../.."))
        .output()
        .expect("Go is required to fetch the pinned kvproto module");
    assert!(
        output.status.success(),
        "kvproto download failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let archive = PathBuf::from(module["Zip"].as_str().expect("module Zip missing"));
    println!("cargo:rerun-if-changed={}", archive.display());
    let bytes = fs::read(archive).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        ARCHIVE_SHA256,
        "kvproto archive checksum mismatch"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let source = out.join("source");
    if source.exists() {
        fs::remove_dir_all(&source).unwrap();
    }
    zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .unwrap()
        .extract(&source)
        .unwrap();
    let root = source.join(format!("github.com/pingcap/kvproto@{VERSION}"));
    let proto = root.join("proto");
    let include = root.join("include");
    let roots = [
        "pdpb",
        "keyspacepb",
        "diagnosticspb",
        "resource_manager",
        "tsopb",
        "schedulingpb",
        "meta_storagepb",
        "routerpb",
    ];
    let files: Vec<_> = roots
        .iter()
        .map(|name| proto.join(format!("{name}.proto")))
        .collect();
    let mut config = tonic_prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path().unwrap());
    tonic_prost_build::configure()
        .include_file("kvproto.rs")
        .file_descriptor_set_path(out.join("kvproto.bin"))
        .compile_with_config(
            config,
            &files,
            &[proto, include, protoc_bin_vendored::include_path().unwrap()],
        )
        .expect("generate pinned kvproto tonic/prost bindings");
}
