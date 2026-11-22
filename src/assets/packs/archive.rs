//! A zip pack read in place, the way vanilla's `java.util.zip.ZipFile`

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use anyhow::{Context, bail, ensure};

/// Larger files are refused, so a broken size field can't ask for gigabytes.
const MAX_ENTRY_SIZE: u64 = 256 << 20;

const END_SIG: u32 = 0x0605_4b50;
const END64_SIG: u32 = 0x0606_4b50;
const END64_LOCATOR_SIG: u32 = 0x0706_4b50;
const CEN_SIG: u32 = 0x0201_4b50;
const LOC_SIG: u32 = 0x0403_4b50;
const END_LEN: usize = 22;
const CEN_LEN: usize = 46;
const LOC_LEN: usize = 30;

/// Where one file's data is.
#[derive(Clone, Copy)]
struct Entry {
    /// Offset of its local header in the file.
    local: u64,
    compressed: u64,
    /// 0 stored, 8 deflated.
    method: u16,
}

pub struct ZipPack {
    file: File,
    /// File name to entry, folders left out.
    index: HashMap<Box<str>, Entry>,
}

impl ZipPack {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let index = read_central_directory(&file, len)?;
        Ok(Self { file, index })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let entry = *self.index.get(name)?;
        let mut header = [0u8; LOC_LEN];
        read_exact_at(&self.file, &mut header, entry.local).ok()?;
        if u32_at(&header, 0) != LOC_SIG {
            return None;
        }
        let skip = u64::from(u16_at(&header, 26)) + u64::from(u16_at(&header, 28));
        let start = entry.local + LOC_LEN as u64 + skip;
        if entry.compressed > MAX_ENTRY_SIZE {
            return None;
        }
        let mut data = vec![0u8; entry.compressed as usize];
        read_exact_at(&self.file, &mut data, start).ok()?;
        match entry.method {
            0 => Some(data),
            8 => {
                let mut out = Vec::with_capacity(data.len() * 3);
                flate2::read::DeflateDecoder::new(data.as_slice())
                    .take(MAX_ENTRY_SIZE + 1)
                    .read_to_end(&mut out)
                    .ok()?;
                (out.len() as u64 <= MAX_ENTRY_SIZE).then_some(out)
            }
            _ => None,
        }
    }

    /// Every file name in the archive.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.index.keys().map(|name| &**name)
    }
}

/// `Source.findEND` and `initCEN`.
fn read_central_directory(file: &File, len: u64) -> anyhow::Result<HashMap<Box<str>, Entry>> {
    ensure!(len >= END_LEN as u64, "zu kurz fuer ein Zip");
    let tail_len = len.min(END_LEN as u64 + 0xFFFF);
    let tail_start = len - tail_len;
    let mut tail = vec![0u8; tail_len as usize];
    read_exact_at(file, &mut tail, tail_start)?;
    let end_at = (0..=tail.len() - END_LEN)
        .rev()
        .find(|&i| u32_at(&tail, i) == END_SIG)
        .context("kein Zip: Endsatz fehlt")?;
    let end_pos = tail_start + end_at as u64;
    let end = &tail[end_at..];
    let mut cen_len = u64::from(u32_at(end, 12));
    let mut cen_off = u64::from(u32_at(end, 16));
    let mut cen_end = end_pos;
    if (cen_len == 0xFFFF_FFFF || cen_off == 0xFFFF_FFFF) && end_pos >= 20 {
        let mut locator = [0u8; 20];
        read_exact_at(file, &mut locator, end_pos - 20)?;
        if u32_at(&locator, 0) == END64_LOCATOR_SIG {
            let end64_pos = u64_at(&locator, 8);
            let mut end64 = [0u8; 56];
            read_exact_at(file, &mut end64, end64_pos)?;
            ensure!(u32_at(&end64, 0) == END64_SIG, "kaputter Zip64-Endsatz");
            cen_len = u64_at(&end64, 40);
            cen_off = u64_at(&end64, 48);
            cen_end = end64_pos;
        }
    }
    let base = cen_end
        .checked_sub(cen_len)
        .and_then(|p| p.checked_sub(cen_off))
        .context("Zentralverzeichnis ausserhalb der Datei")?;
    ensure!(cen_len <= 64 << 20, "Zentralverzeichnis zu gross");
    let mut cen = vec![0u8; cen_len as usize];
    read_exact_at(file, &mut cen, base + cen_off)?;

    let mut index = HashMap::new();
    let mut pos = 0;
    while pos + CEN_LEN <= cen.len() {
        if u32_at(&cen, pos) != CEN_SIG {
            bail!("kaputter Eintrag im Zentralverzeichnis");
        }
        let method = u16_at(&cen, pos + 10);
        let mut compressed = u64::from(u32_at(&cen, pos + 20));
        let mut size = u64::from(u32_at(&cen, pos + 24));
        let name_len = usize::from(u16_at(&cen, pos + 28));
        let extra_len = usize::from(u16_at(&cen, pos + 30));
        let comment_len = usize::from(u16_at(&cen, pos + 32));
        let mut local = u64::from(u32_at(&cen, pos + 42));
        let name_start = pos + CEN_LEN;
        let extra_start = name_start + name_len;
        let next = extra_start + extra_len + comment_len;
        ensure!(
            next <= cen.len(),
            "Eintrag laeuft ueber das Zentralverzeichnis"
        );
        let mut extra = &cen[extra_start..extra_start + extra_len];
        while extra.len() >= 4 {
            let (id, data_len) = (u16_at(extra, 0), usize::from(u16_at(extra, 2)));
            let data = &extra[4..(4 + data_len).min(extra.len())];
            if id == 1 {
                let mut fields = data
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| u64::from_le_bytes(*c));
                for field in [&mut size, &mut compressed, &mut local] {
                    if *field == 0xFFFF_FFFF {
                        *field = fields.next().unwrap_or(*field);
                    }
                }
            }
            extra = &extra[(4 + data_len).min(extra.len())..];
        }
        let _ = size;
        let name: Box<str> = String::from_utf8_lossy(&cen[name_start..extra_start]).into();
        if !name.ends_with('/') {
            index.insert(
                name,
                Entry {
                    local: base + local,
                    compressed,
                    method,
                },
            );
        }
        pos = next;
    }
    index.shrink_to_fit();
    Ok(index)
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap_or_default())
}

fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        let n = read_at(file, buf, offset)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf = &mut buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_stored_and_deflated_entries() {
        let dir = std::env::temp_dir().join(format!("melon-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pack.zip");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.add_directory("assets/", stored).unwrap();
        writer.start_file("pack.mcmeta", stored).unwrap();
        writer.write_all(b"{}").unwrap();
        writer
            .start_file("assets/minecraft/a.txt", deflated)
            .unwrap();
        writer.write_all(&b"hello ".repeat(100)).unwrap();
        writer.finish().unwrap();

        let pack = ZipPack::open(&path).unwrap();
        assert_eq!(pack.read("pack.mcmeta").as_deref(), Some(&b"{}"[..]));
        assert_eq!(pack.read("assets/minecraft/a.txt").unwrap().len(), 600);
        assert!(pack.read("assets/").is_none());
        assert!(pack.read("missing").is_none());
        assert_eq!(pack.names().count(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `MELON_ZIP=<file> cargo test reads_a_real_zip -- --ignored`.
    #[test]
    #[ignore = "needs a zip in MELON_ZIP"]
    fn reads_a_real_zip() {
        let pack = ZipPack::open(Path::new(&std::env::var("MELON_ZIP").unwrap())).unwrap();
        let names: Vec<String> = pack.names().map(str::to_owned).collect();
        let failed: Vec<&String> = names.iter().filter(|n| pack.read(n).is_none()).collect();
        eprintln!(
            "{} entries, {} unreadable {:?}",
            names.len(),
            failed.len(),
            &failed[..failed.len().min(5)]
        );
        assert!(failed.is_empty());
        assert!(pack.read("pack.mcmeta").is_some());
    }

    /// What extraction "protection" does to a server pack.
    #[test]
    fn reads_protected_zips_like_java() {
        let dir = std::env::temp_dir().join(format!("melon-zip-prot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pack.zip");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("pack.mcmeta", deflated).unwrap();
        writer.write_all(&b"{\"pack\":{}}".repeat(10)).unwrap();
        writer.finish().unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let end = bytes.windows(4).rposition(|w| w == b"PK\x05\x06").unwrap();
        bytes[end + 4..end + 6].copy_from_slice(&0xFFFFu16.to_le_bytes());
        bytes[end + 8..end + 12].fill(0);
        let cen = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bytes[cen + 24..cen + 28].copy_from_slice(&0xFFFF_FF7Fu32.to_le_bytes());
        let name_len = usize::from(u16_at(&bytes, 26));
        bytes[4..30].fill(0);
        bytes.drain(30..30 + name_len);
        let shift = name_len as u32;
        let cen = cen - name_len;
        let local = u32_at(&bytes, cen + 42);
        assert_eq!(local, 0);
        let end = end - name_len;
        let cen_off = u32_at(&bytes, end + 16) - shift;
        bytes[end + 16..end + 20].copy_from_slice(&cen_off.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();

        let pack = ZipPack::open(&path).unwrap();
        assert_eq!(pack.read("pack.mcmeta").unwrap().len(), 110);
        std::fs::remove_dir_all(&dir).ok();
    }
}
