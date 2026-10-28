//! Dump an NBT file, or a single chunk of an Anvil region file, as SNBT.
//!
//! ```sh
//! cargo run -p ferrite-nbt --example nbtdump -- level.dat
//! cargo run -p ferrite-nbt --example nbtdump -- r.0.0.mca --chunk 0 0
//! ```
//!
//! Plain files are decompressed by magic bytes (gzip, zlib, or uncompressed NBT).
//! Region files are not NBT, so they need `--chunk <x> <z>` with region-local
//! coordinates: the 8 KiB header's location table is parsed here, then the chunk's
//! length-prefixed payload is decoded with the per-chunk compression byte.

use ferrite_nbt::binary::{self, Compression};
use ferrite_nbt::{Tag, snbt};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SECTOR_BYTES: usize = 4096;
/// Two 4 KiB tables: 1024 chunk locations, then 1024 timestamps.
const HEADER_BYTES: usize = 2 * SECTOR_BYTES;
const USAGE: &str = "\
usage:
  nbtdump <file>                        dump a gzip/zlib/plain NBT file as SNBT
  nbtdump <region.mca> --chunk <x> <z>  dump one chunk (region-local x/z, 0..=31)";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nbtdump: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(args) = Args::parse(std::env::args_os().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let bytes = std::fs::read(&args.file)
        .map_err(|error| format!("cannot read {}: {error}", args.file.display()))?;
    let is_region = args
        .file
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mca"));
    match (is_region, args.chunk) {
        (false, None) => {
            let (name, tag) = decode(&bytes)?;
            print_dump(&name, &tag);
            Ok(())
        }
        (true, Some((x, z))) => dump_chunk(&args.file, &bytes, x, z),
        (true, None) => Err(format!(
            "{} is a region file; pass --chunk <x> <z> to pick a chunk",
            args.file.display()
        )),
        (false, Some(_)) => Err("--chunk only applies to .mca region files".to_string()),
    }
}

fn decode(bytes: &[u8]) -> Result<(String, Tag), String> {
    let compression = detect(bytes);
    binary::read_compressed(bytes, compression).map_err(|error| {
        format!(
            "cannot decode {} NBT: {error}",
            compression_name(compression)
        )
    })
}

/// Sniff the container by magic bytes: gzip (`1f 8b`) or a valid zlib header
/// (low nibble 8, two-byte header divisible by 31), else raw NBT.
fn detect(bytes: &[u8]) -> Compression {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        Compression::Gzip
    } else if let [first, second, ..] = *bytes {
        if first & 0x0f == 0x08 && ((u16::from(first) << 8) | u16::from(second)) % 31 == 0 {
            Compression::Zlib
        } else {
            Compression::None
        }
    } else {
        Compression::None
    }
}

fn compression_name(compression: Compression) -> &'static str {
    match compression {
        Compression::None => "uncompressed",
        Compression::Gzip => "gzip",
        Compression::Zlib => "zlib",
    }
}

fn print_dump(name: &str, tag: &Tag) {
    if name.is_empty() {
        println!("{}", snbt::format(tag));
    } else {
        println!("{name}: {}", snbt::format(tag));
    }
}

fn dump_chunk(path: &Path, region: &[u8], x: u8, z: u8) -> Result<(), String> {
    if region.len() < HEADER_BYTES {
        return Err(format!(
            "{} is {} bytes, too small for an 8 KiB region header",
            path.display(),
            region.len()
        ));
    }
    let index = usize::from(z) * 32 + usize::from(x);
    let entry = &region[index * 4..index * 4 + 4];
    let sector = usize::from(entry[0]) << 16 | usize::from(entry[1]) << 8 | usize::from(entry[2]);
    let sectors = usize::from(entry[3]);
    if sectors == 0 {
        return Err(format!(
            "chunk ({x}, {z}) is not present in {}",
            path.display()
        ));
    }
    if sector < 2 {
        return Err(format!(
            "chunk ({x}, {z}) claims sector {sector}, which overlaps the region header"
        ));
    }
    let start = sector * SECTOR_BYTES;
    let header = region.get(start..start + 5).ok_or_else(|| {
        format!(
            "chunk ({x}, {z}) header at byte {start} is outside {}",
            path.display()
        )
    })?;
    let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let compression = match header[4] {
        1 => Compression::Gzip,
        2 => Compression::Zlib,
        3 => Compression::None,
        other => {
            return Err(format!(
                "chunk ({x}, {z}) uses unsupported compression {other} (expected 1 gzip, 2 zlib, 3 none)"
            ));
        }
    };
    if length == 0 {
        return Err(format!("chunk ({x}, {z}) has an empty payload"));
    }
    if length > sectors * SECTOR_BYTES - 4 {
        return Err(format!(
            "chunk ({x}, {z}) length {length} overflows its {sectors} allocated sectors"
        ));
    }
    let payload = region.get(start + 5..start + 4 + length).ok_or_else(|| {
        format!(
            "chunk ({x}, {z}) payload runs past the end of {}",
            path.display()
        )
    })?;
    let (name, tag) = binary::read_compressed(payload, compression).map_err(|error| {
        format!(
            "cannot decode chunk ({x}, {z}) as {} NBT: {error}",
            compression_name(compression)
        )
    })?;
    print_dump(&name, &tag);
    Ok(())
}

struct Args {
    file: PathBuf,
    chunk: Option<(u8, u8)>,
}

impl Args {
    /// `None` means `--help` was requested.
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut file: Option<PathBuf> = None;
        let mut chunk = None;
        while let Some(arg) = args.next() {
            let arg = arg
                .into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_string())?;
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "--chunk" => {
                    if chunk.is_some() {
                        return Err("--chunk may only be given once".to_string());
                    }
                    let x = coordinate(args.next(), "--chunk x")?;
                    let z = coordinate(args.next(), "--chunk z")?;
                    chunk = Some((x, z));
                }
                flag if flag.len() > 1 && flag.starts_with('-') => {
                    return Err(format!("unknown option {flag:?}\n{USAGE}"));
                }
                path => {
                    if file.is_some() {
                        return Err(format!("unexpected extra path {path:?}\n{USAGE}"));
                    }
                    file = Some(PathBuf::from(path));
                }
            }
        }
        let file = file.ok_or_else(|| format!("missing input file\n{USAGE}"))?;
        Ok(Some(Self { file, chunk }))
    }
}

fn coordinate(arg: Option<OsString>, what: &str) -> Result<u8, String> {
    let raw = arg.ok_or_else(|| format!("{what} is missing after --chunk"))?;
    let raw = raw
        .into_string()
        .map_err(|_| format!("{what} is not valid UTF-8"))?;
    let value: i32 = raw
        .parse()
        .map_err(|_| format!("{what}: {raw:?} is not an integer"))?;
    u8::try_from(value)
        .ok()
        .filter(|coord| *coord < 32)
        .ok_or_else(|| format!("{what} = {value}: chunk coordinates are region-local (0..=31)"))
}
