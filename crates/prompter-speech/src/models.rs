//! Speech models per language: which ones exist, which ship with the app, and
//! downloading the others inside the app (DECISIONS D21).
//!
//! Downloads use the operating system's own `curl` and `tar` (both part of
//! Windows 10/11 and macOS), so the app needs no HTTP or zip library. A download
//! goes to a temporary folder, is checked against the size and SHA-256 below,
//! unpacked, and only then moved into place, so a broken or cancelled download
//! never leaves a half model behind.

use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use prompter_core::Lang;
use sha2::{Digest, Sha256};

/// One Vosk model (all Apache 2.0, from alphacephei.com/vosk/models).
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub lang: Lang,
    /// Archive and folder name, e.g. `vosk-model-small-de-0.15`.
    pub name: &'static str,
    /// Size of the zip in bytes.
    pub zip_bytes: u64,
    pub sha256: &'static str,
    /// Inside the installer; never downloaded or removed.
    pub bundled: bool,
}

pub const CATALOG: [ModelSpec; 6] = [
    ModelSpec {
        lang: Lang::Nl,
        name: "vosk-model-small-nl-0.22",
        zip_bytes: 40_441_176,
        sha256: "039811c3b829de64e4f123a9f684a53784005b212a346ac0b899dc7efce2ed0a",
        bundled: false,
    },
    ModelSpec {
        lang: Lang::En,
        name: "vosk-model-small-en-us-0.15",
        zip_bytes: 41_205_931,
        sha256: "30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498",
        bundled: true,
    },
    ModelSpec {
        lang: Lang::De,
        name: "vosk-model-small-de-0.15",
        zip_bytes: 46_499_967,
        sha256: "b7e53c90b1f0a38456f4cd62b366ecd58803cd97cd42b06438e2c131713d5e43",
        bundled: false,
    },
    ModelSpec {
        lang: Lang::Fr,
        name: "vosk-model-small-fr-0.22",
        zip_bytes: 42_233_323,
        sha256: "cabf6180e177eb9b3a9a9d43a437bd5e549f3a7d09525e5d69a3fed787be12ad",
        bundled: false,
    },
    ModelSpec {
        lang: Lang::Es,
        name: "vosk-model-small-es-0.42",
        zip_bytes: 39_817_833,
        sha256: "09b239888f633ef2f0b4e09736e3d9936acfd810bc65d53fad45261762c6511f",
        bundled: false,
    },
    ModelSpec {
        lang: Lang::Pt,
        name: "vosk-model-small-pt-0.3",
        zip_bytes: 32_453_112,
        sha256: "6e1ce909032e1afa7a88e68a3d628ecafff302bdf195befab308826c395e93b7",
        bundled: false,
    },
];

/// Where the zips are fetched from, in order. The project's own GitHub Release
/// first (so the app normally talks to GitHub only); the Vosk site as fallback.
pub const MIRRORS: [&str; 2] = [
    "https://github.com/bosti-hub/polarityprompt/releases/download/models-1/",
    "https://alphacephei.com/vosk/models/",
];

pub fn spec(lang: Lang) -> &'static ModelSpec {
    CATALOG.iter().find(|s| s.lang == lang).expect("every language has a model")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Progress {
    Downloading { received: u64, total: u64 },
    Verifying,
    Installing,
}

#[derive(Debug)]
pub enum ModelError {
    Cancelled,
    /// Every mirror failed; the message of the last one.
    Download(String),
    Checksum,
    Unpack(String),
    Io(std::io::Error),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Cancelled => write!(f, "download cancelled"),
            ModelError::Download(e) => write!(f, "download failed: {e}"),
            ModelError::Checksum => write!(f, "the downloaded file is damaged (checksum mismatch); please try again"),
            ModelError::Unpack(e) => write!(f, "could not unpack the model: {e}"),
            ModelError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<std::io::Error> for ModelError {
    fn from(e: std::io::Error) -> Self {
        ModelError::Io(e)
    }
}

/// Downloads, verifies and installs `spec` as `<root>/vosk-<code>`. Blocking; run it
/// on its own thread. `cancel` is checked several times a second.
pub fn install(spec: &ModelSpec, root: &Path, cancel: &AtomicBool, mut progress: impl FnMut(Progress)) -> Result<PathBuf, ModelError> {
    let work = root.join(format!(".download-{}", spec.lang.code()));
    if work.exists() {
        fs::remove_dir_all(&work)?;
    }
    fs::create_dir_all(&work)?;
    let result = install_in(spec, root, &work, cancel, &mut progress);
    let _ = fs::remove_dir_all(&work);
    result
}

fn install_in(spec: &ModelSpec, root: &Path, work: &Path, cancel: &AtomicBool, progress: &mut impl FnMut(Progress)) -> Result<PathBuf, ModelError> {
    let zip = work.join(format!("{}.zip", spec.name));
    let mut last_error = String::from("no mirror configured");
    let mut downloaded = false;
    for mirror in MIRRORS {
        let url = format!("{mirror}{}.zip", spec.name);
        match download(&url, &zip, spec.zip_bytes, cancel, progress) {
            Ok(()) => {
                downloaded = true;
                break;
            }
            Err(ModelError::Cancelled) => return Err(ModelError::Cancelled),
            Err(e) => last_error = format!("{url}: {e}"),
        }
    }
    if !downloaded {
        return Err(ModelError::Download(last_error));
    }

    progress(Progress::Verifying);
    if fs::metadata(&zip)?.len() != spec.zip_bytes || sha256_file(&zip)? != spec.sha256 {
        return Err(ModelError::Checksum);
    }

    progress(Progress::Installing);
    let unpacked = work.join("unpacked");
    fs::create_dir_all(&unpacked)?;
    run_tool(Command::new(tar_tool()).arg("-xf").arg(&zip).arg("-C").arg(&unpacked)).map_err(ModelError::Unpack)?;
    let model = unpacked.join(spec.name);
    if !crate::vosk_engine::is_model_dir(&model) {
        return Err(ModelError::Unpack(format!("{} is not a Vosk model", spec.name)));
    }
    let dest = root.join(format!("vosk-{}", spec.lang.code()));
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::rename(&model, &dest)?;
    Ok(dest)
}

/// Removes a downloaded model. Bundled models are never touched.
pub fn remove(lang: Lang, root: &Path) -> std::io::Result<()> {
    if spec(lang).bundled {
        return Err(std::io::Error::other("this language is part of the app and cannot be removed"));
    }
    let dir = root.join(format!("vosk-{}", lang.code()));
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// Runs curl into `dest`, reporting progress from the file size on disk.
fn download(url: &str, dest: &Path, total: u64, cancel: &AtomicBool, progress: &mut impl FnMut(Progress)) -> Result<(), ModelError> {
    let _ = fs::remove_file(dest);
    let mut child = spawn(
        Command::new(curl_tool())
            .args(["--location", "--fail", "--silent", "--show-error", "--retry", "2", "--connect-timeout", "20"])
            .arg("--output")
            .arg(dest)
            .arg(url),
    )?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ModelError::Cancelled);
        }
        let received = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        progress(Progress::Downloading { received, total });
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            return Err(ModelError::Download(stderr_of(&mut child).unwrap_or_else(|| format!("curl exited with {status}"))));
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn run_tool(cmd: &mut Command) -> Result<(), String> {
    let mut child = spawn(cmd).map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(stderr_of(&mut child).unwrap_or_else(|| format!("exited with {status}")))
    }
}

fn spawn(cmd: &mut Command) -> std::io::Result<Child> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // No console window flashing up from a GUI app.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
}

fn stderr_of(child: &mut Child) -> Option<String> {
    let mut text = String::new();
    child.stderr.take()?.read_to_string(&mut text).ok()?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The system's curl, by full path so a curl from Git or elsewhere on PATH is not picked.
fn curl_tool() -> PathBuf {
    system_tool("curl")
}

/// The system's bsdtar, which unpacks zip files (GNU tar from Git does not).
fn tar_tool() -> PathBuf {
    system_tool("tar")
}

fn system_tool(name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        root.join("System32").join(format!("{name}.exe"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/usr/bin").join(name)
    }
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_exactly_one_model() {
        for lang in Lang::ALL {
            assert_eq!(CATALOG.iter().filter(|s| s.lang == lang).count(), 1, "{lang:?}");
            let s = spec(lang);
            assert!(s.name.starts_with(&format!("vosk-model-small-{}", lang.code())), "{}", s.name);
            assert_eq!(s.sha256.len(), 64);
        }
        assert!(spec(Lang::En).bundled);
        assert!(Lang::ALL.iter().filter(|l| spec(**l).bundled).count() == 1, "only English ships with the app");
    }

    #[test]
    fn sha256_matches_a_known_value() {
        let path = std::env::temp_dir().join(format!("pp-sha-{}", std::process::id()));
        fs::write(&path, b"abc").unwrap();
        let hash = sha256_file(&path).unwrap();
        fs::remove_file(&path).ok();
        assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn bundled_models_cannot_be_removed() {
        assert!(remove(Lang::En, Path::new("/nonexistent")).is_err());
    }

    #[test]
    fn system_tools_exist() {
        assert!(curl_tool().exists(), "{}", curl_tool().display());
        assert!(tar_tool().exists(), "{}", tar_tool().display());
    }

    /// Real download of the smallest extra model (40 MB). Run by hand:
    /// `cargo test -p prompter-speech downloads_spanish -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn downloads_spanish() {
        let root = std::env::temp_dir().join(format!("pp-models-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let cancel = AtomicBool::new(false);
        let mut last = None;
        let dir = install(spec(Lang::Es), &root, &cancel, |p| last = Some(p)).unwrap();
        assert!(crate::vosk_engine::is_model_dir(&dir));
        assert_eq!(last, Some(Progress::Installing));
        assert!(!root.join(".download-es").exists());
        let engine = crate::VoskEngine::new(&dir).unwrap();
        assert_eq!(engine.known_words(&["cafe".into(), "hola".into()]).len(), 2);
        fs::remove_dir_all(&root).ok();
    }
}
