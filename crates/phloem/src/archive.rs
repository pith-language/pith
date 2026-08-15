//! POSIX ustar parsing for package source archives.
//!
//! The parser accepts regular files and directories. It rejects links,
//! extension records, unsafe paths, invalid checksums, and truncated input.
//! Importing parsed files into the content store is handled by
//! [`crate::build::unpack`].

const BLOCK: usize = 512;
const NAME: usize = 100;
const SIZE: usize = 124;
const CHKSUM: usize = 148;
const TYPEFLAG: usize = 156;
const MAGIC: usize = 257;
const PREFIX: usize = 345;
const MAGIC_LEN: usize = 5;
const CHKSUM_LEN: usize = 8;
const SIZE_LEN: usize = 12;

const REGULAR: u8 = b'0';
const REGULAR_ALT: u8 = 0;
const DIRECTORY: u8 = b'5';

/// One regular file an archive carried: its path and its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveFile {
    pub path: Box<str>,
    pub bytes: Vec<u8>,
}

/// Parses regular files from a ustar archive in archive order.
///
/// # Errors
/// Returns a diagnostic for malformed archives, unsupported entries, unsafe
/// paths, or repeated file paths.
pub fn parse(bytes: &[u8]) -> pith_diag::PithResult<Box<[ArchiveFile]>> {
    let mut files = Vec::new();
    let mut seen: Vec<Box<str>> = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let header = block_at(bytes, offset)?;
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        check_magic(bytes, offset)?;
        check_checksum(header)?;
        let path = entry_path(bytes, offset)?;
        let size = entry_size(bytes, offset)?;
        let kind = *header
            .get(TYPEFLAG)
            .ok_or_else(|| crate::diag("the header carried no type byte"))?;
        let body = offset
            .checked_add(BLOCK)
            .ok_or_else(|| crate::diag(format!("the entry `{path}` overflows the archive")))?;
        if kind == DIRECTORY {
            offset = next_entry(body, size)
                .ok_or_else(|| crate::diag(format!("the entry `{path}` overflows the archive")))?;
            continue;
        }
        if kind != REGULAR && kind != REGULAR_ALT {
            return Err(crate::diag(format!(
                "the archive entry `{path}` has type byte {kind}, and this reader understands \
                 regular files and directories only"
            )));
        }
        let end = body
            .checked_add(size)
            .ok_or_else(|| crate::diag(format!("the entry `{path}` overflows the archive")))?;
        let data = bytes
            .get(body..end)
            .ok_or_else(|| crate::diag(format!("the entry `{path}` is truncated")))?;
        if seen
            .iter()
            .any(|seen_path| seen_path.as_ref() == path.as_ref())
        {
            return Err(crate::diag(format!(
                "the archive holds two entries named `{path}`, and which one a build reads \
                 would depend on their order"
            )));
        }
        seen.push(path.clone());
        files.push(ArchiveFile {
            path,
            bytes: data.to_vec(),
        });
        offset = next_entry(body, size)
            .ok_or_else(|| crate::diag(format!("the entry at offset {body} overflows")))?;
    }
    Ok(files.into())
}

/// Where the entry after one at `body` with `size` bytes starts, or `None`
/// when that arithmetic overflows.
fn next_entry(body: usize, size: usize) -> Option<usize> {
    body.checked_add(blocks_of(size).checked_mul(BLOCK)?)
}

fn block_at(bytes: &[u8], offset: usize) -> pith_diag::PithResult<&[u8]> {
    let end = offset
        .checked_add(BLOCK)
        .ok_or_else(|| crate::diag("the archive overflows"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| crate::diag("the archive ends inside a header block"))
}

fn check_magic(bytes: &[u8], offset: usize) -> pith_diag::PithResult<()> {
    let start = offset
        .checked_add(MAGIC)
        .ok_or_else(|| crate::diag("the archive overflows"))?;
    let magic = field(bytes, start, MAGIC_LEN)
        .ok_or_else(|| crate::diag("the archive ends inside a header block"))?;
    if magic == b"ustar" {
        return Ok(());
    }
    Err(crate::diag(format!(
        "the block at offset {offset} carries magic `{}`, and this reader parses ustar \
         archives only",
        String::from_utf8_lossy(magic)
    )))
}

/// The header's checksum is the sum of its bytes with the checksum field
/// read as spaces. A corrupted header is refused rather than parsed into
/// whatever the damage left.
fn check_checksum(header: &[u8]) -> pith_diag::PithResult<()> {
    let recorded = match field(header, CHKSUM, CHKSUM_LEN).and_then(octal_field) {
        Some(recorded) => recorded,
        None => return Err(crate::diag("the header carries no checksum")),
    };
    let mut sum: u64 = 0;
    for (index, byte) in header.iter().enumerate() {
        let counted = if (CHKSUM..CHKSUM + CHKSUM_LEN).contains(&index) {
            b' '
        } else {
            *byte
        };
        sum = sum.wrapping_add(u64::from(counted));
    }
    if sum == recorded {
        return Ok(());
    }
    Err(crate::diag(format!(
        "a header records checksum {recorded} and its bytes sum to {sum}"
    )))
}

fn entry_path(bytes: &[u8], offset: usize) -> pith_diag::PithResult<Box<str>> {
    let name = match field(bytes, offset, NAME).and_then(nul_terminated) {
        Some(name) => name,
        None => return Err(crate::diag("the archive ends inside a header block")),
    };
    let prefix_start = offset
        .checked_add(PREFIX)
        .ok_or_else(|| crate::diag("the archive overflows"))?;
    let prefix = match field(bytes, prefix_start, NAME).and_then(nul_terminated) {
        Some(prefix) => prefix,
        None => return Err(crate::diag("the archive ends inside a header block")),
    };
    let joined = if prefix.is_empty() {
        name
    } else {
        format!("{prefix}/{name}")
    };
    if joined.is_empty() {
        return Err(crate::diag("an archive entry carries an empty path"));
    }
    // Only normal relative paths remain inside the package tree.
    let normal = joined
        .split('/')
        .all(|component| !component.is_empty() && component != "." && component != "..");
    if !normal {
        return Err(crate::diag(format!(
            "the archive entry `{joined}` is not a normal relative path, and a file inside \
             the package tree cannot be one that climbs out of it"
        )));
    }
    Ok(joined.into())
}

fn entry_size(bytes: &[u8], offset: usize) -> pith_diag::PithResult<usize> {
    let size_start = offset
        .checked_add(SIZE)
        .ok_or_else(|| crate::diag("the archive overflows"))?;
    let size = match field(bytes, size_start, SIZE_LEN).and_then(octal_field) {
        Some(size) => size,
        None => return Err(crate::diag("an archive header carries no size")),
    };
    usize::try_from(size).map_err(|_| crate::diag("an entry's size does not fit the machine"))
}

/// One NUL-terminated, space-padded header field, as a string. A field
/// with no NUL is read whole, which is the ustar spelling of a name that
/// fills its bytes.
fn nul_terminated(field: &[u8]) -> Option<String> {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8(field.get(..end)?.to_vec()).ok()
}

/// One space- and NUL-padded octal header field.
fn octal_field(field: &[u8]) -> Option<u64> {
    let text: String = field
        .iter()
        .take_while(|byte| **byte != 0 && **byte != b' ')
        .map(|byte| *byte as char)
        .collect();
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(&text, 8).ok()
}

fn field(bytes: &[u8], start: usize, length: usize) -> Option<&[u8]> {
    bytes.get(start..start.checked_add(length)?)
}

fn blocks_of(size: usize) -> usize {
    size.div_ceil(BLOCK)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION_END: usize = 263;
    const OCTAL_LEN: usize = 12;
    const CHECKSUM_LEN: usize = 8;

    /// A ustar header for one regular file, with the checksum computed the
    /// way the reader checks it.
    fn header(path: &str, size: usize, kind: u8) -> [u8; BLOCK] {
        let mut header = [0_u8; BLOCK];
        if let Some(name) = header.get_mut(..path.len()) {
            name.copy_from_slice(path.as_bytes());
        }
        if let Some(magic) = header.get_mut(MAGIC..VERSION_END) {
            magic.copy_from_slice(b"ustar\0");
        }
        if let Some(typeflag) = header.get_mut(TYPEFLAG) {
            *typeflag = kind;
        }
        let octal = format!("{size:011o}\0");
        let size_field = header
            .get_mut(SIZE..SIZE + OCTAL_LEN)
            .and_then(|f| f.get_mut(..octal.len()));
        if let Some(bytes) = size_field {
            bytes.copy_from_slice(octal.as_bytes());
        }
        // Tar checksums treat the checksum field as spaces.
        if let Some(field) = header.get_mut(CHKSUM..CHKSUM + CHECKSUM_LEN) {
            field.fill(b' ');
        }
        let sum: u64 = header.iter().copied().map(u64::from).sum();
        let checksum = format!("{sum:06o}\0 ");
        let field = header
            .get_mut(CHKSUM..CHKSUM + CHECKSUM_LEN)
            .and_then(|field| field.get_mut(..checksum.len()));
        if let Some(bytes) = field {
            bytes.copy_from_slice(checksum.as_bytes());
        }
        header
    }

    fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (path, data) in files {
            bytes.extend_from_slice(&header(path, data.len(), REGULAR));
            bytes.extend_from_slice(data);
            let padding = blocks_of(data.len())
                .checked_mul(BLOCK)
                .unwrap_or_else(|| unreachable!("a fixture file fits the machine"))
                .checked_sub(data.len())
                .unwrap_or_else(|| unreachable!("a fixture file fits one block at least"));
            bytes.extend(std::iter::repeat_n(0, padding));
        }
        bytes.extend_from_slice(&[0_u8; BLOCK * 2]);
        bytes
    }

    #[test]
    fn an_archive_parses_into_its_files_in_archive_order() {
        let bytes = tar(&[
            ("zlib-1.3/zlib.c", b"int zlib(void);\n"),
            ("zlib-1.3/adler32.c", b"int adler(void) { return 0; }\n"),
        ]);
        let files = parse(&bytes).unwrap();
        assert_eq!(files.len(), 2);
        let zlib = files.first().unwrap();
        let adler = files.get(1).unwrap();
        assert_eq!(zlib.path.as_ref(), "zlib-1.3/zlib.c");
        assert_eq!(zlib.bytes, b"int zlib(void);\n");
        assert_eq!(adler.path.as_ref(), "zlib-1.3/adler32.c");
    }

    #[test]
    fn a_file_whose_size_is_exactly_one_block_reads_whole() {
        let data = vec![7_u8; BLOCK];
        let bytes = tar(&[("zlib-1.3/block.bin", &data)]);
        assert_eq!(
            parse(&bytes)
                .unwrap()
                .first()
                .map(|file| file.bytes.clone()),
            Some(data)
        );
    }

    #[test]
    fn directory_entries_carry_no_file() {
        let mut bytes = header("zlib-1.3", 0, DIRECTORY).to_vec();
        bytes.extend(tar(&[("zlib-1.3/zlib.c", b"int zlib(void);\n")]));
        let files = parse(&bytes).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files.first().unwrap().path.as_ref(), "zlib-1.3/zlib.c");
    }

    #[test]
    fn a_corrupted_checksum_is_refused() {
        let mut bytes = tar(&[("zlib-1.3/zlib.c", b"int zlib(void);\n")]);
        if let Some(byte) = bytes.get_mut(10) {
            *byte ^= 0xff;
        }
        let error = parse(&bytes).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("checksum")),
            "the diagnostic names the failed check: {error:?}"
        );
    }

    #[test]
    fn a_truncated_body_is_refused_naming_the_entry() {
        let mut bytes = tar(&[("zlib-1.3/zlib.c", b"int zlib(void);\n")]);
        bytes.truncate(BLOCK + 8);
        let error = parse(&bytes).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("truncated")),
            "the diagnostic names the truncated entry: {error:?}"
        );
    }

    #[test]
    fn an_entry_type_this_reader_refuses_is_named() {
        let mut bytes = tar(&[]);
        let symlink = header("zlib-1.3/link", 0, b'2');
        bytes.splice(..BLOCK, symlink);
        let error = parse(&bytes).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("type byte")),
            "the diagnostic names the refused type: {error:?}"
        );
    }

    #[test]
    fn a_path_that_climbs_out_of_the_tree_is_refused() {
        for path in ["../escape.c", "/absolute.c", "zlib-1.3/../zlib.c", "a//b.c"] {
            let bytes = tar(&[(path, b"int x(void) { return 0; }\n")]);
            let error = parse(&bytes).unwrap_err();
            assert!(
                error.iter().any(|d| d.message.0.contains(path)),
                "the diagnostic names the refused path `{path}`: {error:?}"
            );
        }
    }

    #[test]
    fn a_repeated_path_is_refused_rather_than_order_resolved() {
        let bytes = tar(&[
            ("zlib-1.3/zlib.c", b"first\n"),
            ("zlib-1.3/zlib.c", b"second\n"),
        ]);
        let error = parse(&bytes).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("two entries")),
            "the diagnostic names the repeated path: {error:?}"
        );
    }

    #[test]
    fn bytes_that_are_not_an_archive_are_refused_at_the_magic() {
        let garbage = vec![b'x'; BLOCK * 2];
        let error = parse(&garbage).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("ustar")),
            "the diagnostic names the format expected: {error:?}"
        );

        let error = parse(b"not an archive, just bytes").unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("ends inside") || d.message.0.contains("ustar")),
            "the diagnostic names the truncation or the format: {error:?}"
        );
    }
}
