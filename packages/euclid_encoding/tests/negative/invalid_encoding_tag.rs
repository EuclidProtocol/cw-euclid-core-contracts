//! §9.3: `Encoding::from_u8` is strict (no reserved passthrough); tags other
//! than 0/1 are `InvalidEncodingTag`, and 0/1 roundtrip through `as_u8`.
//!
//! `src/encoding.rs` already carries an in-file `rstest` table covering this
//! exact matrix as a unit test. This integration test duplicates the check
//! deliberately: `Encoding::from_u8`/`as_u8` are the only public entry point
//! a downstream crate (the future contract phase) will call, so pinning the
//! behavior again from outside the crate boundary is the point, not
//! redundant busywork.

use euclid_encoding::{Encoding, EncodingError};

#[test]
fn from_u8_rejects_reserved_tag_two() {
    let err = Encoding::from_u8(2).unwrap_err();
    assert_eq!(err, EncodingError::InvalidEncodingTag { tag: 2 });
}

#[test]
fn from_u8_rejects_tag_255() {
    let err = Encoding::from_u8(255).unwrap_err();
    assert_eq!(err, EncodingError::InvalidEncodingTag { tag: 255 });
}

#[test]
fn from_u8_accepts_json_and_abi_and_roundtrips_as_u8() {
    let json = Encoding::from_u8(0).expect("tag 0 is Json");
    assert_eq!(json, Encoding::Json);
    assert_eq!(json.as_u8(), 0);
    assert_eq!(Encoding::from_u8(json.as_u8()), Ok(json));

    let abi = Encoding::from_u8(1).expect("tag 1 is Abi");
    assert_eq!(abi, Encoding::Abi);
    assert_eq!(abi.as_u8(), 1);
    assert_eq!(Encoding::from_u8(abi.as_u8()), Ok(abi));
}
