//! Canonical-reader and shared-record codec tests.

use pith_core::codec::{
    CanonicalDecodeError, CanonicalReader, encode_bytes, encode_capabilities, encode_content,
    encode_length, encode_str, output_kind_tag, read_capabilities, read_content, read_output_kind,
};
use pith_core::{CapabilityRequirement, Content, OutputKind};
use pith_ids::{ContentDigest, ContentId, DIGEST_LEN};

#[test]
fn read_version_accepts_the_expected_byte() {
    let mut reader = CanonicalReader::new(&[0x03]);
    assert_eq!(reader.read_version(0x03), Ok(()));
    assert_eq!(reader.finish(), Ok(()));
}

#[test]
fn read_version_rejects_a_mismatched_byte() {
    let mut reader = CanonicalReader::new(&[0x02]);
    assert_eq!(
        reader.read_version(0x03),
        Err(CanonicalDecodeError::UnsupportedVersion { version: 0x02 })
    );
}

#[test]
fn read_version_is_truncated_when_empty() {
    let mut reader = CanonicalReader::new(&[]);
    assert_eq!(
        reader.read_version(0x01),
        Err(CanonicalDecodeError::Truncated)
    );
}

#[test]
fn read_byte_is_truncated_when_empty() {
    let mut reader = CanonicalReader::new(&[]);
    assert_eq!(reader.read_byte(), Err(CanonicalDecodeError::Truncated));
}

#[test]
fn read_bool_decodes_zero_and_one() {
    let mut reader = CanonicalReader::new(&[0x00, 0x01]);
    assert_eq!(reader.read_bool(), Ok(false));
    assert_eq!(reader.read_bool(), Ok(true));
    assert_eq!(reader.finish(), Ok(()));
}

#[test]
fn read_bool_rejects_any_byte_beyond_one() {
    for byte in [0x02u8, 0xff, 0x80] {
        let buf = [byte];
        let mut reader = CanonicalReader::new(&buf);
        assert_eq!(
            reader.read_bool(),
            Err(CanonicalDecodeError::InvalidBoolean { byte })
        );
    }
}

#[test]
fn read_bool_is_truncated_when_empty() {
    let mut reader = CanonicalReader::new(&[]);
    assert_eq!(reader.read_bool(), Err(CanonicalDecodeError::Truncated));
}

#[test]
fn read_int_round_trips_boundary_values() {
    for value in [0i64, -1, 1, i64::MIN, i64::MAX, i64::MIN / 2] {
        let encoded = value.to_le_bytes();
        let mut reader = CanonicalReader::new(&encoded);
        assert_eq!(reader.read_int(), Ok(value));
        assert_eq!(reader.finish(), Ok(()));
    }
}

#[test]
fn read_int_is_truncated_for_fewer_than_eight_bytes() {
    for short in 0..=7u8 {
        let encoded = vec![0u8; short as usize];
        let mut reader = CanonicalReader::new(&encoded);
        assert_eq!(reader.read_int(), Err(CanonicalDecodeError::Truncated));
    }
}

#[test]
fn read_length_decodes_a_little_endian_u64() {
    let encoded = 0x0102030405060708u64.to_le_bytes();
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(reader.read_length(), Ok(0x0102030405060708usize));
}

#[test]
fn read_length_is_truncated_for_fewer_than_eight_bytes() {
    let mut reader = CanonicalReader::new(&[0x01, 0x02, 0x03]);
    assert_eq!(reader.read_length(), Err(CanonicalDecodeError::Truncated));
}

#[test]
fn read_bytes_round_trips_empty_and_nonempty_payloads() {
    for payload in [b"" as &[u8], b"abc", &[0u8, 0xff, 0x80][..]] {
        let mut encoded = Vec::new();
        encode_bytes(&mut encoded, payload);
        let mut reader = CanonicalReader::new(&encoded);
        assert_eq!(reader.read_bytes(), Ok(payload));
        assert_eq!(reader.finish(), Ok(()));
    }
}

#[test]
fn read_bytes_is_truncated_when_the_payload_underflows_its_declared_length() {
    let mut encoded = Vec::new();
    encode_length(&mut encoded, 5);
    encoded.extend_from_slice(&[0xaa, 0xbb]);
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(reader.read_bytes(), Err(CanonicalDecodeError::Truncated));
}

#[test]
fn read_bytes_does_not_panic_on_an_absurd_declared_length() {
    // Above `usize::MAX` on 32-bit targets: `LengthOutOfRange`; on 64-bit it is
    // representable and the subsequent `take` fails with `Truncated`.
    let absurd = 0x1_0000_0000u64;
    let encoded = absurd.to_le_bytes();
    let mut reader = CanonicalReader::new(&encoded);
    match reader.read_bytes() {
        Err(CanonicalDecodeError::LengthOutOfRange { length }) => assert_eq!(length, absurd),
        Err(CanonicalDecodeError::Truncated) => {}
        other => unreachable!("unexpected read_bytes result: {other:?}"),
    }
}

#[test]
fn read_text_round_trips_valid_utf8() {
    for text in ["", "ascii", "Pith \u{03bb}\u{00e9}"] {
        let mut encoded = Vec::new();
        encode_str(&mut encoded, text);
        let mut reader = CanonicalReader::new(&encoded);
        assert_eq!(reader.read_text(), Ok(text));
        assert_eq!(reader.finish(), Ok(()));
    }
}

#[test]
fn read_text_rejects_invalid_utf8() {
    let mut encoded = Vec::new();
    encode_length(&mut encoded, 1);
    encoded.push(0xff);
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(reader.read_text(), Err(CanonicalDecodeError::InvalidUtf8));
}

#[test]
fn read_digest_round_trips_thirty_two_bytes() {
    let bytes = [
        0x00u8, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
        0x32, 0x10,
    ];
    let mut reader = CanonicalReader::new(&bytes);
    assert_eq!(reader.read_digest(), Ok(ContentDigest::from_bytes(bytes)));
    assert_eq!(reader.finish(), Ok(()));
}

#[test]
fn read_digest_is_truncated_below_thirty_two_bytes() {
    let short = [0u8; DIGEST_LEN - 1];
    let mut reader = CanonicalReader::new(&short);
    assert_eq!(reader.read_digest(), Err(CanonicalDecodeError::Truncated));
}

#[test]
fn read_content_id_round_trips_and_rejects_truncation() {
    let id = ContentId::of_blob(b"digest payload");
    let digest = id.digest();
    let bytes = digest.as_bytes();
    let mut reader = CanonicalReader::new(bytes);
    assert_eq!(reader.read_content_id(), Ok(id));

    let truncated = &bytes[..DIGEST_LEN - 1];
    let mut reader = CanonicalReader::new(truncated);
    assert_eq!(
        reader.read_content_id(),
        Err(CanonicalDecodeError::Truncated)
    );
}

#[test]
fn take_zero_returns_an_empty_slice_without_consuming() {
    let mut reader = CanonicalReader::new(&[0xaa, 0xbb]);
    assert_eq!(reader.take(0), Ok(&[][..]));
    assert_eq!(reader.read_byte(), Ok(0xaa));
}

#[test]
fn take_is_truncated_when_length_exceeds_remaining() {
    let mut reader = CanonicalReader::new(&[0xaa]);
    assert_eq!(reader.take(2), Err(CanonicalDecodeError::Truncated));
    assert_eq!(reader.read_byte(), Ok(0xaa));
}

#[test]
fn finish_is_ok_when_the_encoding_is_fully_consumed() {
    let mut reader = CanonicalReader::new(&[0xaa]);
    let _ = reader.read_byte();
    assert_eq!(reader.finish(), Ok(()));
}

#[test]
fn read_sequence_accepts_an_empty_length_prefix() {
    let mut encoded = Vec::new();
    encode_length(&mut encoded, 0);
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(reader.read_sequence(|r| r.read_byte()), Ok(Box::default()));
    assert_eq!(reader.finish(), Ok(()));
}

#[test]
fn read_sequence_propagates_a_failure_mid_stream() {
    let mut encoded = Vec::new();
    encode_length(&mut encoded, 3);
    encoded.extend_from_slice(&[0x01, 0x02]);
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(
        reader.read_sequence(|r| r.read_byte()),
        Err(CanonicalDecodeError::Truncated)
    );
}

#[test]
fn output_kind_tag_is_stable_for_blob_and_tree() {
    assert_eq!(output_kind_tag(OutputKind::Blob), 0);
    assert_eq!(output_kind_tag(OutputKind::Tree), 1);
}

#[test]
fn read_output_kind_round_trips_both_variants() {
    for kind in [OutputKind::Blob, OutputKind::Tree] {
        let buf = [output_kind_tag(kind)];
        let mut reader = CanonicalReader::new(&buf);
        assert_eq!(read_output_kind(&mut reader), Ok(kind));
        assert_eq!(reader.finish(), Ok(()));
    }
}

#[test]
fn read_output_kind_rejects_an_unknown_tag() {
    let mut reader = CanonicalReader::new(&[0xff]);
    assert_eq!(
        read_output_kind(&mut reader),
        Err(CanonicalDecodeError::UnknownValueTag { tag: 0xff })
    );
}

#[test]
fn read_output_kind_is_truncated_when_empty() {
    let mut reader = CanonicalReader::new(&[]);
    assert_eq!(
        read_output_kind(&mut reader),
        Err(CanonicalDecodeError::Truncated)
    );
}

#[test]
fn content_round_trips_for_blob_and_tree() {
    let blob = Content::Blob(ContentId::of_blob(b"blob bytes"));
    let tree = Content::Tree(ContentId::of_tree(b"tree manifest"));
    for original in [blob, tree] {
        let mut encoded = Vec::new();
        encode_content(&mut encoded, &original);
        let mut reader = CanonicalReader::new(&encoded);
        assert_eq!(read_content(&mut reader), Ok(original.clone()));
        assert_eq!(reader.finish(), Ok(()));
    }
}

#[test]
fn read_content_rejects_an_unknown_kind_tag() {
    let id = ContentId::of_blob(b"x");
    let mut encoded = vec![0xff];
    encoded.extend_from_slice(id.digest().as_bytes());
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(
        read_content(&mut reader),
        Err(CanonicalDecodeError::UnknownValueTag { tag: 0xff })
    );
}

#[test]
fn read_content_is_truncated_when_the_digest_is_short() {
    let mut encoded = vec![output_kind_tag(OutputKind::Blob)];
    encoded.extend_from_slice(&[0u8; DIGEST_LEN - 1]);
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(
        read_content(&mut reader),
        Err(CanonicalDecodeError::Truncated)
    );
}

#[test]
fn capabilities_round_trip_empty_and_multiple_entries() {
    let cases: &[&[CapabilityRequirement]] = &[
        &[],
        &[CapabilityRequirement {
            name: "net".into(),
            scope: "example.com".into(),
        }],
        &[
            CapabilityRequirement {
                name: "net".into(),
                scope: "example.com".into(),
            },
            CapabilityRequirement {
                name: "fs".into(),
                scope: "/var/lib/pith".into(),
            },
        ],
    ];
    for capabilities in cases {
        let mut encoded = Vec::new();
        encode_capabilities(&mut encoded, capabilities);
        let mut reader = CanonicalReader::new(&encoded);
        let decoded = read_capabilities(&mut reader);
        assert_eq!(
            decoded.as_ref().ok().map(|b| b.as_ref()),
            Some(*capabilities)
        );
        assert_eq!(reader.finish(), Ok(()));
    }
}

#[test]
fn read_capabilities_rejects_a_truncated_scope() {
    let mut encoded = Vec::new();
    encode_length(&mut encoded, 1);
    encode_str(&mut encoded, "net");
    encode_length(&mut encoded, 5);
    encoded.extend_from_slice(b"ab");
    let mut reader = CanonicalReader::new(&encoded);
    assert_eq!(
        read_capabilities(&mut reader),
        Err(CanonicalDecodeError::Truncated)
    );
}

#[test]
fn read_capabilities_does_not_panic_on_an_absurd_scope_length() {
    let absurd = 0x1_0000_0000u64;
    let mut encoded = Vec::new();
    encode_length(&mut encoded, 1);
    encode_str(&mut encoded, "net");
    encoded.extend_from_slice(&absurd.to_le_bytes());
    let mut reader = CanonicalReader::new(&encoded);
    match read_capabilities(&mut reader) {
        Err(CanonicalDecodeError::LengthOutOfRange { length }) => assert_eq!(length, absurd),
        Err(CanonicalDecodeError::Truncated) => {}
        other => unreachable!("unexpected read_capabilities result: {other:?}"),
    }
}
